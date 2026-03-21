// 检测运行环境
export const isDesktopApp = (): boolean => {
  return typeof window !== 'undefined' && '__TAURI__' in window;
};

// 获取默认服务器地址
export const getDefaultServerUrl = (): string => {
  // 桌面应用和 Web 版本都使用相同的默认值
  // 用户可以在设置中自行配置
  return 'ws://localhost:9527';
};

// 获取默认工作目录
export const getDefaultWorkdir = (): string => {
  if (isDesktopApp()) {
    // Tauri 桌面应用：使用用户主目录
    return '~';
  }
  return '/tmp';
};

// 环境信息
export const getEnvironmentInfo = () => {
  return {
    isDesktop: isDesktopApp(),
    platform: typeof window !== 'undefined' ? window.navigator.platform : 'unknown',
    userAgent: typeof window !== 'undefined' ? window.navigator.userAgent : 'unknown',
  };
};
