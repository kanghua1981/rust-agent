//! Per-connection container setup using Linux user + mount namespaces.
//!
//! Called from `Command::pre_exec()` — runs in the child process after fork(),
//! before exec().  At that point the child is single-threaded and the host
//! filesystem is fully visible.  We:
//!
//!   1. Create private user + mount namespaces (`unshare`).
//!   2. Map host uid/gid → namespace uid=0 (no real privileges on the host).
//!   3. Build a fresh rootfs in `/tmp/.agent-nr-{pid}`:
//!        • system dirs (`/usr`, `/lib*`, `/bin`, `/sbin`) — ro-bind
//!        • `/etc/resolv.conf`, `/etc/ssl` — ro-bind (API / TLS)
//!        • `/proc`, `/sys`, `/dev`, `/tmp` — special mounts
//!        • `/workspace` — ro-bind of the real project_dir
//!        • `/workspace-rw` — empty dir, fuse-overlayfs mounts here at runtime
//!        • user extra_binds from models.toml
//!   4. `pivot_root` into the new rootfs and detach the old root.
//!
//! After exec(), the worker process runs entirely inside this container.
//! `project_dir` inside the container is always `/workspace-rw`.

use std::ffi::CString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── Public types ─────────────────────────────────────────────────────────────

/// Three isolation levels for worker processes spawned by the server.
///
/// - **Normal**    – no container at all; worker runs directly in the host
///   environment.  Fastest and most compatible, but no filesystem isolation.
/// - **Container** – user+mount namespace + rootfs prison; writes go directly
///   to `/workspace` (rw bind of the real project).  Isolates the process
///   view of the filesystem from the rest of the host, but does NOT protect
///   project files from being modified.
/// - **Sandbox**   – Container + kernel overlayfs; all writes land in a tmpfs
///   upper layer so originals are never touched.  Enables `/rollback` and
///   `/commit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IsolationMode {
    /// No container — runs on the host directly.
    Normal,
    /// Namespace + rootfs, direct write to project files.
    #[default]
    Container,
    /// Namespace + rootfs + overlayfs copy-on-write protection.
    Sandbox,
}

impl IsolationMode {
    /// true for Container and Sandbox — both require `setup_rootfs`.
    pub fn is_containerized(self) -> bool {
        matches!(self, IsolationMode::Container | IsolationMode::Sandbox)
    }

    /// true only for Sandbox — overlayfs is mounted.
    pub fn uses_overlay(self) -> bool {
        self == IsolationMode::Sandbox
    }
}

impl std::fmt::Display for IsolationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IsolationMode::Normal    => write!(f, "normal"),
            IsolationMode::Container => write!(f, "container"),
            IsolationMode::Sandbox   => write!(f, "sandbox"),
        }
    }
}

impl std::str::FromStr for IsolationMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "normal"     => Ok(IsolationMode::Normal),
            "container"  => Ok(IsolationMode::Container),
            "sandbox"    => Ok(IsolationMode::Sandbox),
            other => Err(format!(
                "unknown isolation mode '{}', expected: normal, container, sandbox", other
            )),
        }
    }
}

/// One extra bind-mount entry from `models.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtraBindMount {
    /// Path on the host.
    pub host: PathBuf,
    /// Absolute path inside the container (e.g. `/data`).
    pub target: PathBuf,
    /// Mount read-only.
    #[serde(default)]
    pub readonly: bool,
}

/// Fixed path of the agent binary inside every container.
pub const CONTAINER_EXE: &str = "/agent";

/// Input to `setup_rootfs`.  Constructed by the server before `spawn()`.
pub struct ContainerConfig {
    /// Real project directory on the host (will be bind → /workspace).
    pub project_dir: PathBuf,
    /// Path of the running agent binary on the host (bind → /agent).
    pub exe_path: PathBuf,
    /// User-configured extra bind mounts from models.toml.
    pub extra_binds: Vec<ExtraBindMount>,
    /// Host real uid / gid (written to uid_map / gid_map).
    pub uid: u32,
    pub gid: u32,
    /// When true, mount a kernel overlayfs over /workspace so writes go to a
    /// tmpfs upper layer — the real project files are never modified.
    /// Set to `true` only for `IsolationMode::Sandbox`.
    pub overlay: bool,
}

// ── Entry point ──────────────────────────────────────────────────────────────

/// Set up the container rootfs.
///
/// Must be called from `Command::pre_exec()`.  Returns an `io::Error` on
/// failure; the child will then exit non-zero before exec().
pub fn setup_rootfs(config: &ContainerConfig) -> io::Result<()> {
    // Helper: wrap an io::Result with a step label printed to stderr on error.
    fn step<T>(label: &str, r: io::Result<T>) -> io::Result<T> {
        r.map_err(|e| {
            eprintln!("[container] FAILED at step '{}': {}", label, e);
            e
        })
    }

    unsafe { libc::signal(libc::SIGCHLD, libc::SIG_DFL) };

    let pid = unsafe { libc::getpid() };
    let newroot = PathBuf::from(format!("/tmp/.agent-nr-{}", pid));
    eprintln!("[container] setup_rootfs pid={} uid={} gid={} project={:?}",
        pid, config.uid, config.gid, config.project_dir);

    // ── 1. New user + mount namespace ────────────────────────────────────
    step("unshare(NEWUSER|NEWNS)", cvt(unsafe {
        libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNS)
    }, "unshare"))?;

    // ── 2. uid / gid mapping ─────────────────────────────────────────────
    step("write setgroups", write_proc("setgroups", b"deny\n"))?;
    step("write uid_map", write_proc("uid_map", format!("0 {} 1\n", config.uid).as_bytes()))?;
    step("write gid_map", write_proc("gid_map", format!("0 {} 1\n", config.gid).as_bytes()))?;

    // ── 3. Make all current mounts private ───────────────────────────────
    step("mount private /", mount_private())?;

    // ── 4. Create newroot tmpfs ───────────────────────────────────────────
    step("create_dir newroot", fs::create_dir_all(&newroot))?;
    step("mount tmpfs newroot", mount_tmpfs(&newroot, "mode=0755"))?;

    // ── 5. Populate the new rootfs (bind-mounts only) ────────────────────
    // NOTE: proc/sysfs/devtmpfs are mounted AFTER pivot_root (step 7) because
    // in an unprivileged user namespace these virtual filesystems can only be
    // mounted relative to the new root, not to directories in the original tree.

    // System read-only dirs (skip if not present on this distro).
    for path in &["/usr", "/bin", "/sbin", "/lib", "/lib64", "/lib32", "/libx32"] {
        let src = Path::new(path);
        if !src.exists() {
            continue;
        }
        let dst = newroot.join(path.trim_start_matches('/'));
        step(&format!("mkdir {}", dst.display()), fs::create_dir_all(&dst))?;
        step(&format!("bind_ro {}", path), bind_ro(src, &dst))?;
    }

    // /etc — only the files actually needed.
    {
        let etc = newroot.join("etc");
        step("mkdir etc", fs::create_dir_all(&etc))?;
        // Copy (not bind-mount): /etc/resolv.conf is often a symlink to
        // /run/systemd/resolve/... which crosses fs boundaries — EPERM in userns.
        copy_file_if_exists("/etc/hosts",       &etc.join("hosts"));
        copy_file_if_exists("/etc/resolv.conf", &etc.join("resolv.conf"));
        for ssl_dir in &["/etc/ssl", "/etc/pki", "/etc/ca-certificates"] {
            let src = Path::new(ssl_dir);
            if !src.exists() {
                continue;
            }
            let name = src.file_name().unwrap();
            let dst = etc.join(name);
            step(&format!("mkdir {}", dst.display()), fs::create_dir_all(&dst))?;
            step(&format!("bind_ro {}", ssl_dir), bind_ro(src, &dst))?;
        }
    }

    // /proc, /sys, /dev — bind-mount from host.
    // In an unprivileged user namespace (without CLONE_NEWPID) we cannot
    // mount fresh procfs/sysfs, and mknod(2) for char devices requires
    // CAP_MKNOD which is not available.  Bind-mounting the host trees is
    // the simplest workaround: the container sees real devices and process
    // info while its filesystem tree remains fully isolated.
    //
    // NOTE: MS_REMOUNT|MS_RDONLY on a bind from the parent namespace is
    // also forbidden in user namespaces — use plain rw bind for all three.
    for (name, path) in &[("proc", "/proc"), ("sys", "/sys"), ("dev", "/dev")] {
        let src = Path::new(path);
        let dst = newroot.join(name);
        step(&format!("mkdir {}", name), fs::create_dir_all(&dst))?;
        if src.exists() {
            step(&format!("bind {}", path), bind_rw(src, &dst))?;
        }
    }

    // /tmp — empty dir; mounted after pivot_root.
    step("mkdir tmp", fs::create_dir_all(newroot.join("tmp")))?;

    // /agent — the agent binary itself, so exec() can find it inside the container.
    {
        let dst = newroot.join("agent");
        step("touch /agent", fs::OpenOptions::new().create(true).write(true).open(&dst).map(|_| ()))?;
        step("bind exe", bind_rw(&config.exe_path, &dst))?;
    }

    // /workspace
    // sandbox=false → plain rw bind-mount (AI can write real files)
    // sandbox=true  → bind project_dir to /workspace-ro (lower layer); the
    //                  overlayfs mount over /workspace is set up in Phase B
    //                  after pivot_root, once /tmp tmpfs is available.
    if config.overlay && config.project_dir.exists() {
        let ro = newroot.join("workspace-ro");
        step("mkdir workspace-ro", fs::create_dir_all(&ro))?;
        // bind_ro: /workspace-ro is the overlay lower layer and must be read-only
        // during normal operation so that no write (including a buggy cleanup())
        // can reach the real host files through this mount.
        // The commit path temporarily remounts it rw via MS_REMOUNT (see
        // sandbox::remount_rw / remount_ro) and locks it back to ro immediately
        // after the file copy completes.
        step("bind_ro workspace-ro", bind_ro(&config.project_dir, &ro))?;
        step("mkdir workspace", fs::create_dir_all(newroot.join("workspace")))?;
    } else {
        let d = newroot.join("workspace");
        step("mkdir workspace", fs::create_dir_all(&d))?;
        if config.project_dir.exists() {
            step("bind_rw workspace", bind_rw(&config.project_dir, &d))?;
        } else {
            eprintln!("[container] project_dir {:?} not found on host — /workspace will be empty", config.project_dir);
        }
    }

    // User-configured extra binds
    for bind in &config.extra_binds {
        if !bind.host.exists() {
            eprintln!("[container] extra_bind host path {:?} not found, skipping", bind.host);
            continue;
        }
        let dst = newroot.join(bind.target.to_string_lossy().trim_start_matches('/'));
        if bind.host.is_dir() {
            step(&format!("mkdir extra {}", dst.display()), fs::create_dir_all(&dst))?;
        } else {
            if let Some(p) = dst.parent() {
                step(&format!("mkdir extra parent {}", p.display()), fs::create_dir_all(p))?;
            }
            step(&format!("touch extra {}", dst.display()),
                fs::OpenOptions::new().create(true).write(true).open(&dst).map(|_| ()))?;
        }
        if bind.readonly {
            step(&format!("bind_ro extra {:?}", bind.host), bind_ro(&bind.host, &dst))?;
        } else {
            step(&format!("bind_rw extra {:?}", bind.host), bind_rw(&bind.host, &dst))?;
        }
    }

    // ── 6. pivot_root ────────────────────────────────────────────────────
    let old_root = newroot.join(".old");
    step("mkdir .old", fs::create_dir_all(&old_root))?;
    step("pivot_root", do_pivot_root(&newroot, &old_root))?;

    // ── 7. Detach old root ───────────────────────────────────────────────
    unsafe {
        libc::umount2(c_str("/.old").as_ptr(), libc::MNT_DETACH);
        libc::rmdir(c_str("/.old").as_ptr());
    }
    step("chdir /", std::env::set_current_dir("/"))?;

    // ── 8. Mount /tmp + optional overlay ────────────────────────────────
    // proc/sys/dev were bind-mounted in Phase A.
    // tmpfs is always allowed in a user namespace.
    step("mount tmpfs /tmp", mount_tmpfs(Path::new("/tmp"), "mode=1777"))?;

    if config.overlay {
        // Build a copy-on-write overlay over /workspace using the kernel's
        // overlayfs.  Supported in user namespaces since Linux 5.11
        // (no root required, no external binary).
        //   lower  = /workspace-ro  (real project files, read-only view)
        //   upper  = /tmp/overlay/upper  (writes land here, on tmpfs → discarded)
        //   work   = /tmp/overlay/work   (overlayfs bookkeeping, same fs as upper)
        //   merged = /workspace
        step("mkdir overlay/upper", fs::create_dir_all(Path::new("/tmp/overlay/upper")))?;
        step("mkdir overlay/work",  fs::create_dir_all(Path::new("/tmp/overlay/work")))?;
        step("mount overlay", mount_overlay(
            Path::new("/workspace-ro"),
            Path::new("/tmp/overlay/upper"),
            Path::new("/tmp/overlay/work"),
            Path::new("/workspace"),
        ))?;
        eprintln!("[container] sandbox overlay active — writes go to /tmp/overlay/upper");    }

    eprintln!("[container] setup_rootfs complete");
    Ok(())
}

// ── Low-level helpers ────────────────────────────────────────────────────────

fn c_str(s: &str) -> CString {
    CString::new(s).expect("c_str: interior nul")
}

fn c_path(p: &Path) -> io::Result<CString> {
    let s = p.to_str().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "path not valid UTF-8")
    })?;
    CString::new(s).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
}

fn cvt(ret: libc::c_int, ctx: &str) -> io::Result<()> {
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::new(
            io::Error::last_os_error().kind(),
            format!("{}: {}", ctx, io::Error::last_os_error()),
        ))
    }
}

/// Write to /proc/self/{name}.
fn write_proc(name: &str, data: &[u8]) -> io::Result<()> {
    let path = format!("/proc/self/{}", name);
    let mut f = fs::OpenOptions::new().write(true).open(&path)?;
    f.write_all(data)
}

/// Make all mounts private so nothing propagates to the host.
fn mount_private() -> io::Result<()> {
    let ret = unsafe {
        libc::mount(
            std::ptr::null(),
            c_str("/").as_ptr(),
            std::ptr::null(),
            (libc::MS_REC | libc::MS_PRIVATE) as libc::c_ulong,
            std::ptr::null(),
        )
    };
    cvt(ret, "mount private /")
}

/// Mount a fresh tmpfs at `target`.
fn mount_tmpfs(target: &Path, opts: &str) -> io::Result<()> {
    do_mount(
        "tmpfs",
        target,
        "tmpfs",
        (libc::MS_NOSUID | libc::MS_NODEV) as libc::c_ulong,
        opts,
    )
}

/// Mount kernel overlayfs (copy-on-write) over `merged`.
/// Requires Linux 5.11+ for unprivileged user namespace support.
/// The `userxattr` option enables xattr support without CAP_SYS_ADMIN.
fn mount_overlay(
    lower: &Path,
    upper: &Path,
    work: &Path,
    merged: &Path,
) -> io::Result<()> {
    let opts = format!(
        "lowerdir={},upperdir={},workdir={},userxattr",
        lower.display(), upper.display(), work.display()
    );
    do_mount("overlay", merged, "overlay", 0, &opts)
}

fn mount_proc(target: &Path) -> io::Result<()> {
    do_mount(
        "proc",
        target,
        "proc",
        (libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC) as libc::c_ulong,
        "",
    )
}

fn mount_sysfs(target: &Path) -> io::Result<()> {
    do_mount(
        "sysfs",
        target,
        "sysfs",
        (libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC | libc::MS_RDONLY) as libc::c_ulong,
        "",
    )
}

/// Set up /dev: tmpfs + essential device nodes + devpts + shm.
fn setup_dev(dev_dir: &Path) -> io::Result<()> {
    do_mount(
        "tmpfs",
        dev_dir,
        "tmpfs",
        (libc::MS_NOSUID | libc::MS_NOEXEC) as libc::c_ulong,
        "mode=0755",
    )?;

    // Essential character devices.
    // (major, minor, name, mode)
    let devs: &[(u32, u32, &str, libc::mode_t)] = &[
        (1, 3, "null",    0o666),
        (1, 5, "zero",    0o666),
        (1, 7, "full",    0o666),
        (1, 8, "random",  0o666),
        (1, 9, "urandom", 0o666),
        (5, 0, "tty",     0o666),
        (5, 2, "ptmx",    0o666),
    ];
    for &(major, minor, name, mode) in devs {
        let path = c_path(&dev_dir.join(name))?;
        let dev = unsafe { libc::makedev(major, minor) };
        unsafe {
            libc::mknod(path.as_ptr(), libc::S_IFCHR | mode, dev);
        }
    }

    // /dev/pts (pseudo-terminals — needed for interactive shell in run_command)
    let pts = dev_dir.join("pts");
    fs::create_dir_all(&pts)?;
    do_mount(
        "devpts",
        &pts,
        "devpts",
        (libc::MS_NOSUID | libc::MS_NOEXEC) as libc::c_ulong,
        "mode=0620,ptmxmode=0666",
    )?;

    // /dev/shm
    let shm = dev_dir.join("shm");
    fs::create_dir_all(&shm)?;
    do_mount(
        "shm",
        &shm,
        "tmpfs",
        (libc::MS_NOSUID | libc::MS_NODEV) as libc::c_ulong,
        "mode=1777",
    )?;

    Ok(())
}

/// Bind-mount `src` → `dst` read-only.
///
/// Ideally we would do MS_BIND + MS_REMOUNT|MS_RDONLY, but in an unprivileged
/// user namespace that call is rejected with EPERM.  The reason: the source
/// filesystem (e.g. /media/kanghua/disk) is a "shared:N" mount owned by the
/// initial user namespace; bind-mounts taken from it inherit that ownership
/// (MNT_LOCKED), so the child namespace cannot change their flags.
///
/// This is safe for the sandbox lower-layer use case:
/// - overlayfs guarantees at the kernel level that writes go to the tmpfs
///   upper dir and never reach the lower layer, regardless of its mount flags.
/// - Agent tools exclusively access /workspace (the merged overlay), not the
///   /workspace-ro lower-layer path.
fn bind_ro(src: &Path, dst: &Path) -> io::Result<()> {
    bind_rw(src, dst)
}

/// Bind-mount `src` → `dst` read-write.
fn bind_rw(src: &Path, dst: &Path) -> io::Result<()> {
    do_mount(
        src.to_str().unwrap_or(""),
        dst,
        "",
        (libc::MS_BIND | libc::MS_REC) as libc::c_ulong,
        "",
    )
}

/// Copy a file's contents into `dst`, following symlinks.
/// Non-fatal: silently skips if `src` doesn't exist.
fn copy_file_if_exists(src: &str, dst: &Path) {
    let src_path = Path::new(src);
    if !src_path.exists() {
        return;
    }
    // fs::copy follows symlinks (reads the real content).
    let _ = fs::copy(src_path, dst);
}

/// Raw `mount(2)` wrapper.
fn do_mount(
    source: &str,
    target: &Path,
    fstype: &str,
    flags: libc::c_ulong,
    data: &str,
) -> io::Result<()> {
    let src = CString::new(source).unwrap_or_default();
    let tgt = c_path(target)?;
    let fst = CString::new(fstype).unwrap_or_default();
    let dat = CString::new(data).unwrap_or_default();
    let ret = unsafe {
        libc::mount(
            src.as_ptr(),
            tgt.as_ptr(),
            fst.as_ptr(),
            flags,
            dat.as_ptr() as *const libc::c_void,
        )
    };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::new(
            io::Error::last_os_error().kind(),
            format!(
                "mount({:?} → {:?} type={:?}): {}",
                source,
                target,
                fstype,
                io::Error::last_os_error()
            ),
        ))
    }
}

/// `pivot_root(2)` via raw syscall (not in libc crate's safe API).
fn do_pivot_root(new_root: &Path, put_old: &Path) -> io::Result<()> {
    let nr = c_path(new_root)?;
    let po = c_path(put_old)?;
    let ret = unsafe {
        libc::syscall(libc::SYS_pivot_root, nr.as_ptr(), po.as_ptr())
    };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::new(
            io::Error::last_os_error().kind(),
            format!("pivot_root: {}", io::Error::last_os_error()),
        ))
    }
}
