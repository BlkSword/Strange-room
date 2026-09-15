//! 设备簿：记住"上次连过谁"。
//!
//! 「零准备」和「验证码对一眼」是给**第一次**见面准备的。第二次见面还让人对码，
//! 就是把本该产品做的事推回给用户。设备簿是"最近的人"这一层最简形态：
//! 只在本机记 指纹 + 名字 + 时间 + 次数，不联网、不上传。
//!
//! # 边界（这些必须说清楚）
//!
//! - **只有接收端记得住主机**。主机拿不到访客的证书（没做 mTLS），它只知道对方
//!   在握手时报的名字——那名字不构成身份。所以设备簿是单向的：访客记住主机。
//! - **指纹仍然来自二维码/连接串**（带外信道）。设备簿只是"记住"，不是"替代验证"：
//!   连接时的指纹校验一次都没少。
//! - **同一个名字换了指纹要报警**。那可能是对方重装了（换了自签证书），
//!   也可能是有人顶着同样的名字在中间。两种情况都该让用户看一眼再决定。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fs_util;
use crate::identity::Identity;

/// 设备簿文件名（放在用户数据目录里，和设备身份同一个目录）。
const FILE: &str = "known_devices.json";

/// 最多记多少台。设备簿是便利功能，不能变成一个无限增长的文件；
/// 超出时丢掉最久没见的那些。
const MAX_DEVICES: usize = 200;

/// 一台见过的设备。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownDevice {
    /// 证书指纹（hex）。这是身份本身，来自二维码/连接串。
    pub fingerprint: String,
    /// 对方当时报的设备名。**只是显示用，不是身份**。
    pub name: String,
    pub first_seen_ms: u64,
    /// 最近一次连上是什么时候
    pub last_seen_ms: u64,
    /// 一共成功连过几次
    pub connects: u32,
}

/// 本机的设备簿。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceBook {
    #[serde(default)]
    pub devices: Vec<KnownDevice>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl DeviceBook {
    pub fn path() -> PathBuf {
        Identity::default_dir().join(FILE)
    }

    /// 读本机设备簿。读不懂就当空——它只是便利功能，坏掉也绝不能让传输失败。
    pub fn load() -> Self {
        Self::load_from(&Self::path())
    }

    pub fn load_from(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// 写回本机设备簿（先写临时文件再原子改名，半截文件不会覆盖好的那份）。
    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path())
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        }
        let json = serde_json::to_vec_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| Error::io(&tmp, e))?;
        fs_util::atomic_rename(&tmp, path)
    }

    pub fn find(&self, fingerprint: &str) -> Option<&KnownDevice> {
        self.devices
            .iter()
            .find(|d| d.fingerprint.eq_ignore_ascii_case(fingerprint))
    }

    pub fn is_known(&self, fingerprint: &str) -> bool {
        self.find(fingerprint).is_some()
    }

    /// 名字相同、指纹不同：这是要提醒用户的信号（重装？还是别人顶着这个名字？）。
    pub fn name_taken_by_other(&self, name: &str, fingerprint: &str) -> Option<&KnownDevice> {
        self.devices
            .iter()
            .find(|d| d.name == name && !d.fingerprint.eq_ignore_ascii_case(fingerprint))
    }

    /// 记一次"成功连上"。
    pub fn remember(&mut self, fingerprint: &str, name: &str) {
        let now = now_ms();
        if let Some(d) = self
            .devices
            .iter_mut()
            .find(|d| d.fingerprint.eq_ignore_ascii_case(fingerprint))
        {
            // 名字有可能被对方改过：跟着更新，但身份（指纹）不变
            d.name = name.to_string();
            d.last_seen_ms = now;
            d.connects = d.connects.saturating_add(1);
            return;
        }

        self.devices.push(KnownDevice {
            fingerprint: fingerprint.to_string(),
            name: name.to_string(),
            first_seen_ms: now,
            last_seen_ms: now,
            connects: 1,
        });
        self.trim();
    }

    /// 忘掉一台设备。返回是否真的删掉了。
    pub fn forget(&mut self, fingerprint: &str) -> bool {
        let before = self.devices.len();
        self.devices
            .retain(|d| !d.fingerprint.eq_ignore_ascii_case(fingerprint));
        self.devices.len() != before
    }

    /// 最近连过的排前面——设备簿的用处就是"最近的人"。
    pub fn recent(&self) -> Vec<&KnownDevice> {
        let mut out: Vec<&KnownDevice> = self.devices.iter().collect();
        out.sort_by(|a, b| b.last_seen_ms.cmp(&a.last_seen_ms));
        out
    }

    fn trim(&mut self) {
        if self.devices.len() <= MAX_DEVICES {
            return;
        }
        self.devices.sort_by(|a, b| b.last_seen_ms.cmp(&a.last_seen_ms));
        self.devices.truncate(MAX_DEVICES);
    }
}

/// 把毫秒时间戳写成"多久以前"，给人看。
pub fn human_age(last_seen_ms: u64, now_ms: u64) -> String {
    let secs = now_ms.saturating_sub(last_seen_ms) / 1000;
    match secs {
        0..=59 => "刚刚".to_string(),
        60..=3599 => format!("{} 分钟前", secs / 60),
        3600..=86_399 => format!("{} 小时前", secs / 3600),
        _ => format!("{} 天前", secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("chuan-trust-{}-{}.json", name, std::process::id()))
    }

    #[test]
    fn remember_adds_then_updates_without_duplicating() {
        let mut book = DeviceBook::default();
        book.remember("aa11", "小黑的笔记本");
        assert_eq!(book.devices.len(), 1);
        assert_eq!(book.devices[0].connects, 1);
        assert!(book.is_known("AA11"), "指纹比较应当忽略大小写");

        // 同名同指纹再来一次：是同一次关系，不是新设备
        book.remember("aa11", "小黑的笔记本");
        assert_eq!(book.devices.len(), 1, "同一个指纹不该记成两台设备");
        assert_eq!(book.devices[0].connects, 2);

        // 对方改了名字：跟着更新名字，身份不变
        book.remember("aa11", "小黑的台式机");
        assert_eq!(book.devices[0].name, "小黑的台式机");
        assert_eq!(book.devices[0].connects, 3);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let path = tmp_path("roundtrip");
        let mut book = DeviceBook::default();
        book.remember("deadbeef", "张三的手机");
        book.save_to(&path).expect("保存失败");

        let loaded = DeviceBook::load_from(&path);
        assert_eq!(loaded.devices.len(), 1);
        assert_eq!(loaded.devices[0].fingerprint, "deadbeef");
        assert_eq!(loaded.devices[0].name, "张三的手机");
        assert_eq!(loaded.devices[0].connects, 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_or_missing_file_is_just_empty() {
        let path = tmp_path("corrupt");
        let _ = std::fs::remove_file(&path);
        assert!(DeviceBook::load_from(&path).devices.is_empty(), "文件不存在=空");

        std::fs::write(&path, b"{ not json").unwrap();
        assert!(
            DeviceBook::load_from(&path).devices.is_empty(),
            "读不懂就当空：设备簿坏了不能挡住传输"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn same_name_with_a_different_fingerprint_is_flagged() {
        let mut book = DeviceBook::default();
        book.remember("aaaa", "会议室那台");
        assert!(book.name_taken_by_other("会议室那台", "bbbb").is_some(), "换了指纹要报警");
        assert!(book.name_taken_by_other("会议室那台", "aaaa").is_none(), "同一个指纹不算异常");
        assert!(book.name_taken_by_other("别的名字", "bbbb").is_none());
    }

    #[test]
    fn forget_and_recent() {
        let mut book = DeviceBook::default();
        book.remember("aaaa", "A");
        book.remember("bbbb", "B");
        // 让 A 看起来更近
        book.devices[0].last_seen_ms = now_ms();
        book.devices[1].last_seen_ms = 1;
        assert_eq!(book.recent()[0].name, "A", "最近见过的排前面");

        assert!(book.forget("aaaa"));
        assert!(!book.forget("aaaa"), "删过了就不该再报删成功");
        assert_eq!(book.devices.len(), 1);
    }

    #[test]
    fn book_is_capped() {
        let mut book = DeviceBook::default();
        for i in 0..(MAX_DEVICES + 20) {
            book.devices.push(KnownDevice {
                fingerprint: format!("fp{i}"),
                name: format!("device{i}"),
                first_seen_ms: 0,
                last_seen_ms: i as u64,
                connects: 1,
            });
        }
        book.remember("new", "new");
        assert!(book.devices.len() <= MAX_DEVICES, "设备簿不能无限长");
        assert!(book.is_known("new"), "新记的不能被裁掉");
    }

    #[test]
    fn human_age_reads_naturally() {
        let now = 10_000_000_000u64;
        assert_eq!(human_age(now, now), "刚刚");
        assert_eq!(human_age(now - 90_000, now), "1 分钟前");
        assert_eq!(human_age(now - 3 * 3_600_000, now), "3 小时前");
        assert_eq!(human_age(now - 5 * 86_400_000, now), "5 天前");
    }
}
