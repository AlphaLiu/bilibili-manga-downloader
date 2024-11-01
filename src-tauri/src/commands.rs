use crate::bili_client::BiliClient;
use crate::config::Config;
use crate::download_manager::DownloadManager;
use crate::errors::CommandResult;
use crate::extensions::IgnoreRwLockPoison;
use crate::responses::{
    BiliResp, Buvid3RespData, GenerateQrcodeRespData, MangaRespData, QrcodeStatusRespData,
    SearchMangaRespData, UserProfileRespData,
};
use crate::types::{EpisodeInfo, Manga, QrcodeData, QrcodeStatus};
use anyhow::{anyhow, Context};
use base64::engine::general_purpose;
use base64::Engine;
use image::Rgb;
use path_slash::PathBufExt;
use qrcode::QrCode;
use reqwest::StatusCode;
use serde_json::{from_str, json};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::RwLock;
use tauri::{AppHandle, State};

#[tauri::command]
#[specta::specta]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn get_config(config: State<RwLock<Config>>) -> Config {
    config.read().unwrap().clone()
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn save_config(
    app: AppHandle,
    config_state: State<RwLock<Config>>,
    config: Config,
) -> CommandResult<()> {
    let mut config_state = config_state.write_or_panic();
    *config_state = config;
    config_state.save(&app)?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn generate_qrcode(bili_client: State<'_, BiliClient>) -> CommandResult<QrcodeData> {
    let qrcode_data = bili_client.generate_qrcode().await?;
    Ok(qrcode_data)
}

#[tauri::command(async)]
#[specta::specta]
pub async fn get_qrcode_status(
    bili_client: State<'_, BiliClient>,
    auth_code: String,
) -> CommandResult<QrcodeStatus> {
    let qrcode_status = bili_client.get_qrcode_status(auth_code).await?;
    Ok(qrcode_status)
}

#[tauri::command(async)]
#[specta::specta]
pub async fn get_user_profile(
    bili_client: State<'_, BiliClient>,
) -> CommandResult<UserProfileRespData> {
    let user_profile_resp_data = bili_client.get_user_profile().await?;
    Ok(user_profile_resp_data)
}

#[tauri::command(async)]
#[specta::specta]
pub async fn get_buvid3() -> CommandResult<Buvid3RespData> {
    // 发送获取buvid3请求
    let http_resp = reqwest::Client::new()
        .get("https://api.bilibili.com/x/web-frontend/getbuvid")
        .send()
        .await?;
    // 检查http响应状态码
    let status = http_resp.status();
    let body = http_resp.text().await?;
    if status != StatusCode::OK {
        return Err(anyhow!("获取buvid3失败，预料之外的状态码({status}): {body}").into());
    }
    // 尝试将body解析为BiliResp
    let bili_resp = from_str::<BiliResp>(&body)
        .context(format!("获取buvid3失败，将body解析为BiliResp失败: {body}"))?;
    // 检查BiliResp的code字段
    if bili_resp.code != 0 {
        return Err(anyhow!("获取buvid3失败，预料之外的code: {bili_resp:?}").into());
    }
    // 检查BiliResp的data是否存在
    let Some(data) = bili_resp.data else {
        return Err(anyhow!("获取buvid3失败，data字段不存在: {bili_resp:?}").into());
    };
    // 尝试将data解析为Buvid3RespData
    let data_str = data.to_string();
    let buvid3_resp_data = from_str::<Buvid3RespData>(&data_str).context(format!(
        "获取buvid3失败，将data解析为Buvid3RespData失败: {data_str}"
    ))?;

    Ok(buvid3_resp_data)
}

#[tauri::command(async)]
#[specta::specta]
pub async fn search_manga(
    config: State<'_, RwLock<Config>>,
    keyword: &str,
    page_num: i64,
) -> CommandResult<SearchMangaRespData> {
    let cookie = config.read_or_panic().get_cookie();
    let payload = json!({
        "key_word": keyword,
        "page_num": page_num,
        "page_size": 20,
    });
    // 发送搜索漫画请求
    let http_resp = reqwest::Client::new()
        .post("https://manga.bilibili.com/twirp/comic.v1.Comic/Search?device=pc&platform=web")
        .header("user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36")
        .header("cookie", &cookie)
        .json(&payload)
        .send()
        .await?;
    // 检查http响应状态码
    let status = http_resp.status();
    let body = http_resp.text().await?;
    if status != StatusCode::OK {
        return Err(anyhow!("搜索漫画失败，预料之外的状态码({status}): {body}").into());
    }
    // 尝试将body解析为BiliResp
    let bili_resp =
        from_str::<BiliResp>(&body).context(format!("将body解析为BiliResp失败: {body}"))?;
    // 检查BiliResp的code字段
    if bili_resp.code != 0 {
        return Err(anyhow!("搜索漫画失败，预料之外的code: {bili_resp:?}").into());
    }
    // 检查BiliResp的data是否存在
    let Some(data) = bili_resp.data else {
        return Err(anyhow!("搜索漫画失败，data字段不存在: {bili_resp:?}").into());
    };
    // 尝试将data解析为SearchMangaRespData
    let data_str = data.to_string();
    let search_manga_resp_data = from_str::<SearchMangaRespData>(&data_str).context(format!(
        "搜索漫画失败，将data解析为SearchMangaRespData失败: {data_str}"
    ))?;

    Ok(search_manga_resp_data)
}

#[tauri::command(async)]
#[specta::specta]
pub async fn get_manga(
    app: AppHandle,
    config: State<'_, RwLock<Config>>,
    id: i64,
) -> CommandResult<Manga> {
    let cookie = config.read_or_panic().get_cookie();
    let payload = json!({"comic_id": id});
    // 发送获取漫画详情请求
    let http_res = reqwest::Client::new()
        .post("https://manga.bilibili.com/twirp/comic.v1.Comic/ComicDetail?device=pc&platform=web")
        .header("user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36")
        .header("cookie", &cookie)
        .json(&payload)
        .send()
        .await?;
    // 检查http响应状态码
    let status = http_res.status();
    let body = http_res.text().await?;
    if status != StatusCode::OK {
        return Err(anyhow!("获取漫画详情失败，预料之外的状态码({status}): {body}").into());
    }
    // 尝试将body解析为BiliResp
    let bili_resp = from_str::<BiliResp>(&body).context(format!(
        "获取漫画详情失败，将body解析为BiliResp失败: {body}"
    ))?;
    // 检查BiliResp的code字段
    if bili_resp.code != 0 {
        return Err(anyhow!("获取漫画详情失败，预料之外的code: {bili_resp:?}").into());
    }
    // 检查BiliResp的data是否存在
    let Some(data) = bili_resp.data else {
        return Err(anyhow!("获取漫画详情失败，data字段不存在: {bili_resp:?}").into());
    };
    // 尝试将data解析为MangaRespData
    let data_str = data.to_string();
    let manga_resp_data = from_str::<MangaRespData>(&data_str).context(format!(
        "获取漫画详情失败，将data解析为MangaRespData失败: {data_str}"
    ))?;
    let manga = Manga::from_manga_resp_data(&app, manga_resp_data);

    Ok(manga)
}

#[tauri::command(async)]
#[specta::specta]
pub async fn download_episodes(
    download_manager: State<'_, DownloadManager>,
    episodes: Vec<EpisodeInfo>,
) -> CommandResult<()> {
    for ep in episodes {
        download_manager.submit_episode(ep).await?;
    }
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub fn show_path_in_file_manager(path: &str) -> CommandResult<()> {
    let path = PathBuf::from_slash(path);
    if !path.exists() {
        return Err(anyhow!("路径`{path:?}`不存在").into());
    }
    showfile::show_path_in_file_manager(path);
    Ok(())
}
