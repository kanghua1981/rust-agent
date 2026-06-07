// Allow CSS-in-JS to use the non-standard WebkitAppRegion property
// needed for frameless window drag regions in Tauri.
declare namespace React {
  interface CSSProperties {
    WebkitAppRegion?: string;
  }
}
