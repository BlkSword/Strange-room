/**
 * 工具函数 - 基于昵称生成唯一ID
 */

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
 * 生成基于昵称的稳定ID（同一昵称总是得到相同的ID）
 * @param nickname 用户昵称
 * @param roomId 房间ID（用于在同一房间内保证唯一性）
 * @returns 稳定的唯一ID
 */
export function generateStableIdFromNickname(nickname: string, roomId: string): string {
  // 组合昵称和房间ID进行哈希
  const combined = `${nickname}_${roomId}`;

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
