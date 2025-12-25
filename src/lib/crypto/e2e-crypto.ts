/**
 * 端到端加密模块
 * 使用 Web Crypto API 实现 AES-GCM 加密
 * 服务器无法解密数据，只有拥有房间密钥的客户端可以
 */

export class E2EEncryption {
  private key: CryptoKey | null = null;
  private keyId: string | null = null;

  /**
   * 生成新的房间密钥
   */
  async generateRoomKey(): Promise<string> {
    // 生成 256 位密钥
    this.key = await crypto.subtle.generateKey(
      {
        name: 'AES-GCM',
        length: 256,
      },
      true,
      ['encrypt', 'decrypt']
    );

    // 导出原始密钥用于分享
    const exportedKey = await crypto.subtle.exportKey('raw', this.key);
    const keyArray = Array.from(new Uint8Array(exportedKey));
    const keyHex = keyArray.map(b => b.toString(16).padStart(2, '0')).join('');

    this.keyId = this.generateKeyId();
    return `${this.keyId}:${keyHex}`;
  }

  /**
   * 从导入的密钥初始化
   */
  async importRoomKey(keyString: string): Promise<void> {
    const [keyId, keyHex] = keyString.split(':');

    const keyArray = new Uint8Array(keyHex.match(/.{2}/g)!.map(b => parseInt(b, 16)));
    this.key = await crypto.subtle.importKey(
      'raw',
      keyArray,
      { name: 'AES-GCM', length: 256 },
      true,
      ['encrypt', 'decrypt']
    );

    this.keyId = keyId;
  }

  /**
   * 加密数据
   */
  async encrypt(data: any): Promise<{ encrypted: string; keyId: string }> {
    if (!this.key) {
      throw new Error('No encryption key available');
    }

    const dataStr = JSON.stringify(data);
    const encoder = new TextEncoder();
    const iv = crypto.getRandomValues(new Uint8Array(12)); // 96 位 IV

    const encrypted = await crypto.subtle.encrypt(
      { name: 'AES-GCM', iv },
      this.key,
      encoder.encode(dataStr)
    );

    // 将 IV 和加密数据合并
    const combined = new Uint8Array(iv.length + encrypted.byteLength);
    combined.set(iv);
    combined.set(new Uint8Array(encrypted));

    // 转换为 base64
    const encryptedBase64 = btoa(String.fromCharCode(...combined));
    return { encrypted: encryptedBase64, keyId: this.keyId! };
  }

  /**
   * 解密数据
   */
  async decrypt<T = any>(encryptedBase64: string): Promise<T | null> {
    if (!this.key) {
      throw new Error('No decryption key available');
    }

    try {
      // 从 base64 解析
      const combined = Uint8Array.from(atob(encryptedBase64), c => c.charCodeAt(0));

      // 提取 IV 和加密数据
      const iv = combined.slice(0, 12);
      const encrypted = combined.slice(12);

      const decrypted = await crypto.subtle.decrypt(
        { name: 'AES-GCM', iv },
        this.key,
        encrypted
      );

      const decoder = new TextDecoder();
      const decryptedStr = decoder.decode(decrypted);
      return JSON.parse(decryptedStr) as T;
    } catch (error) {
      console.error('[E2E] 解密失败:', error);
      return null;
    }
  }

  /**
   * 检查是否有密钥
   */
  hasKey(): boolean {
    return this.key !== null;
  }

  /**
   * 生成密钥 ID
   */
  private generateKeyId(): string {
    return 'key_' + crypto.getRandomValues(new Uint32Array(1))[0].toString(36);
  }
}

/**
 * 房间密钥管理
 */
export class RoomKeyManager {
  private encryption = new E2EEncryption();
  private sharedKeys: Map<string, CryptoKey> = new Map(); // roomId -> key

  /**
   * 创建新房间并生成密钥
   */
  async createRoom(): Promise<{ keyId: string; keyString: string }> {
    const keyString = await this.encryption.generateRoomKey();
    return {
      keyId: keyString.split(':')[0],
      keyString,
    };
  }

  /**
   * 加入房间并导入密钥
   */
  async joinRoom(keyString: string): Promise<void> {
    await this.encryption.importRoomKey(keyString);
  }

  /**
   * 加密要发送的数据
   */
  async encryptMessage(data: any): Promise<{ encrypted: string; keyId: string }> {
    return await this.encryption.encrypt(data);
  }

  /**
   * 解密收到的数据
   */
  async decryptMessage<T = any>(encryptedBase64: string): Promise<T | null> {
    return await this.encryption.decrypt<T>(encryptedBase64);
  }

  /**
   * 检查是否有密钥
   */
  hasKey(): boolean {
    return this.encryption.hasKey();
  }
}
