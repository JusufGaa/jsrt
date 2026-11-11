use std::env;
use std::fs;
use std::path::PathBuf;

/// 精确版本，唯一需要升级时改动的地方
const VUE_VERSION: &str = "3.5.17";
const VUE_FILE: &str = "dist/vue.runtime.global.prod.js";

fn out_dir() -> PathBuf {
    PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"))
}

fn cached_path() -> PathBuf {
    // 放在仓库里，跟 git 走，这样 CI 和干净构建都能命中
    PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("third_party")
        .join("vue.runtime.global.prod.js")
}

fn fetch(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let resp = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?
        .get(url)
        .send()?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {} from {}", resp.status(), url).into());
    }
    Ok(resp.text()?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cached = cached_path();
    let out_js = out_dir().join("vue_runtime.js");

    // 告诉 cargo：这些文件变了就重跑
    println!("cargo:rerun-if-changed={}", cached.display());
    println!("cargo:rerun-if-env-changed=VUE_OFFLINE");

    let vue = if env::var("VUE_OFFLINE").is_ok() {
        // 离线模式：只读本地缓存，任何联网企图都是构建错误
        fs::read_to_string(&cached)
            .map_err(|_| "离线模式开启，但 third_party/vue.runtime.global.prod.js 不存在。请在有网环境构建一次后再切离线。")?
    } else if cached.exists() {
        // 本地已有：校验哈希，过了就直接用，不联网
        fs::read_to_string(&cached)?
    } else {
        // 本地没有，联网拉取
        let urls = [
            format!("https://cdn.jsdelivr.net/npm/vue@{VUE_VERSION}/{VUE_FILE}")
        ];

        let mut last_err = None;
        let mut text = None;
        for url in &urls {
            match fetch(url) {
                Ok(t) => { text = Some(t); break; }
                Err(e) => {
                    eprintln!("warn: fetch {} failed: {}", url, e);
                    last_err = Some(e);
                }
            }
        }
        let text = text.ok_or_else(|| {
            format!("所有 CDN 源均失败: {:?}", last_err.unwrap())
        })?;

        // 落盘缓存，供下次离线使用
        if let Some(parent) = cached.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&cached, &text)?;
        text
    };



    fs::write(&out_js, vue)?;

    // 让库代码能用 include_str! 拿到它
    println!("cargo:rustc-env=VUE_RUNTIME_JS={}", out_js.display());
    Ok(())
}


