// Tauri API 类型声明
declare global {
  interface Window {
    __TAURI__: {
      invoke: <T>(command: string, args?: any) => Promise<T>;
      dialog: any;
      fs: any;
      path: any;
      process: any;
      shell: any;
    };
  }
}

// 测试 Tauri API 是否可用
export const testTauriApi = async () => {
  if (typeof window !== 'undefined' && window.__TAURI__) {
    try {
      // 测试简单的 greet 命令
      const result = await window.__TAURI__.invoke<string>('greet', { name: 'Rust Agent' });
      console.log('Tauri API 测试成功:', result);
      return true;
    } catch (error) {
      console.error('Tauri API 测试失败:', error);
      return false;
    }
  }
  return false;
};

// 获取用户主目录
export const getHomeDirectory = async (): Promise<string> => {
  if (typeof window !== 'undefined' && window.__TAURI__) {
    try {
      return await window.__TAURI__.invoke<string>('get_home_dir');
    } catch (error) {
      console.error('获取主目录失败:', error);
      return '~';
    }
  }
  return '~';
};

// 在外部编辑器中打开文件（仅 Tauri 桌面环境有效）
// 返回 true 表示已通过 Tauri 处理；返回 false 表示需走 WebSocket 回退
export const openFileExternal = async (path: string): Promise<boolean> => {
  if (typeof window !== 'undefined' && window.__TAURI__) {
    try {
      await window.__TAURI__.invoke<void>('open_file_external', { path });
      return true;
    } catch (error) {
      console.error('Tauri open_file_external 失败:', error);
      return false;
    }
  }
  return false;
};