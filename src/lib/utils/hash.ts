/**
 * 工具函数 - 基于昵称和设备指纹生成唯一ID
 */

/**
 * 获取设备指纹
 * 通过收集浏览器和设备特征生成一个相对稳定的指纹
 * @returns 设备指纹字符串
 */
export function getDeviceFingerprint(): string {
  // 尝试从 localStorage 获取已保存的指纹
  const STORAGE_KEY = 'strange_room_device_fp';

  if (typeof window !== 'undefined') {
    try {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (saved) {
        return saved;
      }
    } catch (e) {
      // localStorage 可能不可用
    }

    // 生成新的设备指纹
    const components: string[] = [];

    // 1. 屏幕信息
    if (window.screen) {
      components.push(`${window.screen.width}x${window.screen.height}`);
      components.push(`${window.screen.colorDepth}`);
    }

    // 2. 时区信息
    if (Intl.DateTimeFormat) {
      try {
        const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
        components.push(tz || '');
      } catch (e) {
        // ignore
      }
    }

    // 3. 语言信息
    if (navigator.language) {
      components.push(navigator.language);
    }

    // 4. 平台信息
    if (navigator.platform) {
      components.push(navigator.platform);
    }

    // 5. User Agent (截取主要部分)
    if (navigator.userAgent) {
      // 提取浏览器版本号的主要部分
      const uaMatch = navigator.userAgent.match(/(chrome|safari|firefox|edg|opr)\/([\d.]+)/i);
      if (uaMatch) {
        components.push(`${uaMatch[1]}${uaMatch[2]}`);
      }
    }

    // 6. Canvas 指纹（简化版）
    try {
      const canvas = document.createElement('canvas');
      const ctx = canvas.getContext('2d');
      if (ctx) {
        ctx.textBaseline = 'top';
        ctx.font = '14px Arial';
        ctx.fillText('Fingerprint', 2, 2);
        const canvasData = canvas.toDataURL().slice(-50); // 取后50个字符
        components.push(canvasData);
      }
    } catch (e) {
      // Canvas 可能不可用
    }

    // 7. WebGL 指纹（简化版）
    try {
      const canvas = document.createElement('canvas');
      const gl = canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
      if (gl) {
        const debugInfo = gl.getExtension('WEBGL_debug_renderer_info');
        if (debugInfo) {
          const renderer = gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL);
          components.push(renderer || '');
        }
      }
    } catch (e) {
      // WebGL 可能不可用
    }

    // 8. 触摸点数量
    if (navigator.maxTouchPoints !== undefined) {
      components.push(`touch${navigator.maxTouchPoints}`);
    }

    // 9. 硬件并发数
    if (navigator.hardwareConcurrency) {
      components.push(`cores${navigator.hardwareConcurrency}`);
    }

    // 10. 设备内存（如果可用）
    if ((navigator as any).deviceMemory) {
      components.push(`ram${(navigator as any).deviceMemory}`);
    }

    // 组合所有组件并生成哈希
    const combined = components.join('|');

    // 使用 FNV-1a 哈希算法生成指纹
    let hash = 2166136261;
    for (let i = 0; i < combined.length; i++) {
      const char = combined.charCodeAt(i);
      hash ^= char;
      hash += (hash << 1) + (hash << 4) + (hash << 7) + (hash << 8) + (hash << 24);
      hash >>>= 0;
    }

    const fingerprint = hash.toString(36).padStart(8, '0');

    // 保存到 localStorage
    try {
      localStorage.setItem(STORAGE_KEY, fingerprint);
    } catch (e) {
      // ignore
    }

    return fingerprint;
  }

  // 如果在服务端，返回空字符串
  return '';
}

/**
 * 使用简单的字符串哈希算法生成唯一ID
 * @param nickname 用户昵称
 * @returns 基于昵称的唯一ID
 */
export function generateIdFromNickname(nickname: string): string {
  // 使用 FNV-1a 哈希算法的变体
  let hash = 2166136261; // FNV offset basis

  for (let i = 0; i < nickname.length; i++) {
    const char = nickname.charCodeAt(i);
    hash ^= char;
    hash += (hash << 1) + (hash << 4) + (hash << 7) + (hash << 8) + (hash << 24);
    hash >>>= 0; // 确保是无符号整数
  }

  // 转换为 36 进制并添加时间戳部分以确保唯一性（同一昵称在不同时间可能需要区分）
  const timestamp = Date.now();
  return `user_${hash.toString(36)}_${timestamp}`;
}

/**
 * 生成基于昵称和设备指纹的唯一ID
 * 同一台计算机 + 同一昵称 = 相同ID
 * 不同计算机 + 同一昵称 = 不同ID
 *
 * @param nickname 用户昵称
 * @param roomId 房间ID（用于区分不同房间）
 * @returns 唯一的用户ID
 */
export function generateStableIdFromNickname(nickname: string, roomId: string): string {
  // 处理空昵称：使用默认昵称
  const safeNickname = (nickname || '匿名用户').trim();

  // 昵称不能为空，如果为空则使用默认值
  if (!safeNickname) {
    return `user_${Math.random().toString(36).substring(2, 10)}`;
  }

  // 获取设备指纹
  const deviceFp = getDeviceFingerprint();

  // 组合昵称、房间ID和设备指纹进行哈希
  const combined = `${safeNickname}_${roomId}_${deviceFp}`;

  // 使用 FNV-1a 哈希算法
  let hash = 2166136261;

  for (let i = 0; i < combined.length; i++) {
    const char = combined.charCodeAt(i);
    hash ^= char;
    hash += (hash << 1) + (hash << 4) + (hash << 7) + (hash << 8) + (hash << 24);
    hash >>>= 0;
  }

  // 转换为 36 进制字符串，取前 8 位作为 ID
  const hashStr = hash.toString(36).padStart(8, '0').slice(0, 8);

  return `user_${hashStr}`;
}

/**
 * 验证两个昵称是否对应同一个用户ID
 * @param nickname1 第一个昵称
 * @param nickname2 第二个昵称
 * @param roomId 房间ID
 * @returns 是否对应同一个用户ID
 */
export function isSameUser(nickname1: string, nickname2: string, roomId: string): boolean {
  return generateStableIdFromNickname(nickname1, roomId) === generateStableIdFromNickname(nickname2, roomId);
}
