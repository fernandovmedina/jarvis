//! Hardware readings straight from /proc and /sys.

use std::ffi::{CStr, CString};
use std::fs;
use std::net::Ipv4Addr;
use std::time::Instant;

#[derive(Clone, Copy, Default)]
struct CpuTimes {
    total: u64,
    idle: u64,
}

#[derive(Default)]
pub struct Sys {
    prev_cpu: Vec<CpuTimes>,
    /// [0] = all cores, then one per core, in percent.
    pub cpu: Vec<f64>,
    pub mem_total: u64,
    pub mem_avail: u64,
    pub swap_total: u64,
    pub swap_free: u64,
    pub temps: Vec<(String, f64)>,
    pub load: [f64; 3],
    pub uptime: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub ips: Vec<(String, Ipv4Addr)>,
    prev_net: Option<(u64, u64, Instant)>,
    pub net_iface: String,
    pub rx_rate: f64,
    pub tx_rate: f64,
    pub battery: Option<(u32, String)>,
    pub ac: Option<bool>,
    pub hostname: String,
}

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

impl Sys {
    /// Every second: cpu, memory, temperature, load, uptime, network rate.
    pub fn fast(&mut self) {
        self.read_cpu();
        self.read_mem();
        self.read_temps();
        self.read_net();
        let l = read("/proc/loadavg");
        for (i, v) in l.split_whitespace().take(3).enumerate() {
            self.load[i] = v.parse().unwrap_or(0.0);
        }
        self.uptime = read("/proc/uptime")
            .split_whitespace()
            .next()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0) as u64;
    }

    /// Every 10 seconds: disk, addresses, battery, hostname.
    pub fn slow(&mut self) {
        self.hostname = read("/proc/sys/kernel/hostname").trim().to_string();
        self.read_disk();
        self.read_ips();
        self.read_power();
    }

    fn read_cpu(&mut self) {
        let now: Vec<CpuTimes> = read("/proc/stat")
            .lines()
            .take_while(|l| l.starts_with("cpu"))
            .map(|l| {
                let v: Vec<u64> = l.split_whitespace().skip(1).take(8).filter_map(|x| x.parse().ok()).collect();
                CpuTimes {
                    total: v.iter().sum(),
                    idle: v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0),
                }
            })
            .collect();
        if self.prev_cpu.len() == now.len() {
            self.cpu = now
                .iter()
                .zip(&self.prev_cpu)
                .map(|(n, p)| {
                    let dt = n.total.saturating_sub(p.total);
                    let di = n.idle.saturating_sub(p.idle);
                    if dt == 0 { 0.0 } else { 100.0 * (dt - di.min(dt)) as f64 / dt as f64 }
                })
                .collect();
        } else {
            self.cpu = vec![0.0; now.len()];
        }
        self.prev_cpu = now;
    }

    fn read_mem(&mut self) {
        for l in read("/proc/meminfo").lines() {
            let mut it = l.split_whitespace();
            let (Some(k), Some(v)) = (it.next(), it.next()) else { continue };
            let v: u64 = v.parse().unwrap_or(0);
            match k {
                "MemTotal:" => self.mem_total = v,
                "MemAvailable:" => self.mem_avail = v,
                "SwapTotal:" => self.swap_total = v,
                "SwapFree:" => self.swap_free = v,
                _ => {}
            }
        }
    }

    fn read_temps(&mut self) {
        let mut temps = Vec::new();
        if let Ok(dir) = fs::read_dir("/sys/class/hwmon") {
            for e in dir.flatten() {
                let p = e.path();
                if fs::read_to_string(p.join("name")).unwrap_or_default().trim() != "coretemp" {
                    continue;
                }
                for i in 1..=32 {
                    let Ok(v) = fs::read_to_string(p.join(format!("temp{i}_input"))) else { continue };
                    let Ok(v) = v.trim().parse::<f64>() else { continue };
                    let label = fs::read_to_string(p.join(format!("temp{i}_label"))).unwrap_or_default();
                    let label = label.trim();
                    let short = if label.starts_with("Package") {
                        "pkg".to_string()
                    } else if let Some(n) = label.strip_prefix("Core ") {
                        format!("c{n}")
                    } else {
                        format!("t{i}")
                    };
                    temps.push((short, v / 1000.0));
                }
            }
        }
        if temps.is_empty() {
            // fallback: thermal zones with a sane reading (acpitz on this box reports -269 C)
            if let Ok(dir) = fs::read_dir("/sys/class/thermal") {
                for e in dir.flatten() {
                    let p = e.path();
                    let Ok(v) = fs::read_to_string(p.join("temp")) else { continue };
                    let Ok(v) = v.trim().parse::<f64>() else { continue };
                    if v > 0.0 && v < 150_000.0 {
                        let t = fs::read_to_string(p.join("type")).unwrap_or_default();
                        temps.push((t.trim().to_string(), v / 1000.0));
                    }
                }
            }
        }
        self.temps = temps;
    }

    fn read_net(&mut self) {
        if self.net_iface.is_empty() {
            return;
        }
        let mut cur = None;
        for l in read("/proc/net/dev").lines().skip(2) {
            let Some((name, rest)) = l.split_once(':') else { continue };
            if name.trim() == self.net_iface {
                let f: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                if f.len() >= 9 {
                    cur = Some((f[0], f[8]));
                }
            }
        }
        let Some((rx, tx)) = cur else { return };
        let now = Instant::now();
        if let Some((prx, ptx, pt)) = self.prev_net {
            let dt = (now - pt).as_secs_f64().max(0.001);
            self.rx_rate = rx.saturating_sub(prx) as f64 / dt;
            self.tx_rate = tx.saturating_sub(ptx) as f64 / dt;
        }
        self.prev_net = Some((rx, tx, now));
    }

    fn read_disk(&mut self) {
        let path = CString::new("/").unwrap();
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(path.as_ptr(), &mut st) } == 0 {
            let fr = st.f_frsize as u64;
            self.disk_total = st.f_blocks as u64 * fr;
            self.disk_used = self.disk_total.saturating_sub(st.f_bfree as u64 * fr);
        }
    }

    fn read_ips(&mut self) {
        let mut ips = Vec::new();
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        if unsafe { libc::getifaddrs(&mut ifap) } == 0 {
            let mut cur = ifap;
            while !cur.is_null() {
                let ifa = unsafe { &*cur };
                if !ifa.ifa_addr.is_null() && unsafe { (*ifa.ifa_addr).sa_family } as i32 == libc::AF_INET {
                    let sin = unsafe { &*(ifa.ifa_addr as *const libc::sockaddr_in) };
                    let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                    let name = unsafe { CStr::from_ptr(ifa.ifa_name) }.to_string_lossy().into_owned();
                    if !ip.is_loopback() {
                        ips.push((name, ip));
                    }
                }
                cur = ifa.ifa_next;
            }
            unsafe { libc::freeifaddrs(ifap) };
        }
        // Physical interfaces first (wlp*/enp*/eth*), virtual ones (docker, veth) after.
        ips.sort_by_key(|(n, _)| !(n.starts_with('w') || n.starts_with('e')));
        if let Some((n, _)) = ips.first() {
            if *n != self.net_iface {
                self.net_iface = n.clone();
                self.prev_net = None;
            }
        }
        self.ips = ips;
    }

    fn read_power(&mut self) {
        self.battery = None;
        self.ac = None;
        let Ok(dir) = fs::read_dir("/sys/class/power_supply") else { return };
        for e in dir.flatten() {
            let p = e.path();
            let kind = fs::read_to_string(p.join("type")).unwrap_or_default();
            match kind.trim() {
                "Battery" => {
                    let cap = fs::read_to_string(p.join("capacity")).unwrap_or_default();
                    let status = fs::read_to_string(p.join("status")).unwrap_or_default();
                    if let Ok(c) = cap.trim().parse() {
                        self.battery = Some((c, status.trim().to_string()));
                    }
                }
                "Mains" => {
                    let on = fs::read_to_string(p.join("online")).unwrap_or_default();
                    self.ac = Some(on.trim() == "1");
                }
                _ => {}
            }
        }
    }
}
