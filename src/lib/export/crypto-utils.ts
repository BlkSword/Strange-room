/**
 * 导出数据加密/解密工具
 * 使用 CryptoJS 实现 AES-256-CBC 加密
 */

import CryptoJS from 'crypto-js';
import { EncryptedExportData, RoomExportData } from '@/types/export';

/**
 * 生成随机盐值
 */
function generateSalt(): string {
  return CryptoJS.lib.WordArray.random(128 / 8).toString(CryptoJS.enc.Base64);
}

/**
 * 生成随机初始化向量
 */
function generateIV(): string {
  return CryptoJS.lib.WordArray.random(128 / 8).toString(CryptoJS.enc.Base64);
}

/**
 * 从密码派生密钥 (PBKDF2)
 */
function deriveKey(password: string, salt: string): CryptoJS.lib.WordArray {
  const saltWordArray = CryptoJS.enc.Base64.parse(salt);
  return CryptoJS.PBKDF2(password, saltWordArray, {
    keySize: 256 / 32, // 256位密钥
    iterations: 100000, // 迭代次数
  });
}

/**
 * 加密导出数据
 */
export function encryptExportData(
  data: RoomExportData,
  password: string
): EncryptedExportData {
  // 生成盐值和IV
  const salt = generateSalt();
  const iv = generateIV();

  // 派生密钥
  const key = deriveKey(password, salt);

  // 将数据转换为JSON字符串
  const jsonData = JSON.stringify(data);

  // 使用 AES 加密
  const encrypted = CryptoJS.AES.encrypt(jsonData, key, {
    iv: CryptoJS.enc.Base64.parse(iv),
    mode: CryptoJS.mode.CBC,
    padding: CryptoJS.pad.Pkcs7,
  });

  return {
    version: '1.0',
    encrypted: true,
    iv,
    salt,
    data: encrypted.toString(),
  };
}

/**
 * 解密导出数据
 */
export function decryptExportData(
  encryptedData: EncryptedExportData,
  password: string
): RoomExportData {
  try {
    // 派生密钥
    const key = deriveKey(password, encryptedData.salt);

    // 解密数据
    const decrypted = CryptoJS.AES.decrypt(encryptedData.data, key, {
      iv: CryptoJS.enc.Base64.parse(encryptedData.iv),
      mode: CryptoJS.mode.CBC,
      padding: CryptoJS.pad.Pkcs7,
    });

    // 转换为UTF-8字符串
    const jsonString = decrypted.toString(CryptoJS.enc.Utf8);

    if (!jsonString) {
      throw new Error('解密失败：密码错误或数据损坏');
    }

    // 解析JSON
    return JSON.parse(jsonString) as RoomExportData;
  } catch (error) {
    throw new Error('解密失败：' + (error as Error).message);
  }
}

/**
 * 验证密码（不解密全部数据，只尝试验证）
 */
export function verifyPassword(
  encryptedData: EncryptedExportData,
  password: string
): boolean {
  try {
    decryptExportData(encryptedData, password);
    return true;
  } catch {
    return false;
  }
}
