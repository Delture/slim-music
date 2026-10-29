//! 网易云接口封装：基于 crates.io 的 ncm-api-rs（WTFPL），只保留本应用需要的几个接口。
use crate::model::{LyricLine, LyricPayload, Playlist, Profile, Source, Track};
use ncm_api_rs::{create_client, ApiResponse, Query};
use serde_json::Value;

type R<T> = Result<T, String>;

fn ok(res: Result<ApiResponse, ncm_api_rs::NcmError>) -> R<ApiResponse> {
    let res = res.map_err(|e| e.to_string())?;
    if res.status != 200 {
        let msg = res.body.get("message").or(res.body.get("msg")).and_then(Value::as_str).unwrap_or("请求失败");
        return Err(format!("网易云返回 {}: {}", res.status, msg));
    }
    Ok(res)
}

fn q(cookie: &Option<String>) -> Query {
    let q = Query::new();
    match cookie {
        Some(c) => q.cookie(c),
        None => q,
    }
}

/// 把网易云歌曲 JSON（song/detail、cloudsearch、推荐等通用格式）转换为 Track
pub fn to_track(v: &Value) -> Option<Track> {
    let id = v.get("id")?.as_i64()?;
    let al = v.get("al").or(v.get("album"));
    let ar = v.get("ar").or(v.get("artists"));
    Some(Track {
        id: format!("ncm:{id}"),
        source: Source::Netease,
        title: v.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
        artists: ar
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.get("name")?.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        album: al.and_then(|a| a.get("name")).and_then(Value::as_str).unwrap_or("").to_string(),
        duration_ms: v.get("dt").or(v.get("duration")).and_then(Value::as_u64).unwrap_or(0),
        cover: al.and_then(|a| a.get("picUrl")).and_then(Value::as_str).map(|s| format!("{s}?param=200y200")),
        path: None,
        ncm_id: Some(id),
    })
}

fn tracks_at(v: &Value) -> Vec<Track> {
    v.as_array().map(|a| a.iter().filter_map(to_track).collect()).unwrap_or_default()
}

/// 把 Set-Cookie 列表压成 "k=v; k2=v2" 形式，供后续请求使用
fn join_set_cookie(list: &[String]) -> String {
    list.iter()
        .filter_map(|c| c.split(';').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

pub struct QrKey {
    pub key: String,
    pub svg: String,
}


fn normalize_qr_svg(raw: &str, size: u32) -> String {
    // 覆盖 width/height，保留 viewBox，保证按正方形缩放
    let mut tag_end = None;
    let start = raw.find("<svg").unwrap_or(0);
    for (i, ch) in raw[start..].char_indices() {
        if ch == '>' {
            tag_end = Some(start + i);
            break;
        }
    }
    let Some(end) = tag_end else { return raw.to_string() };
    let mut tag = raw[start..=end].to_string();
    for attr in ["width", "height"] {
        let key = format!("{attr}=\"");
        if let Some(a) = tag.find(&key) {
            let val_start = a + key.len();
            if let Some(rel) = tag[val_start..].find('"') {
                tag.replace_range(a..val_start + rel + 1, &format!("{attr}=\"{size}\""));
            }
        } else {
            tag = tag.replacen("<svg", &format!("<svg {attr}=\"{size}\""), 1);
        }
    }
    format!("{}{}{}", &raw[..start], tag, &raw[end + 1..])
}

pub async fn qr_key() -> R<QrKey> {
    let c = create_client(None);
    let res = ok(c.login_qr_key(&Query::new()).await)?;
    let key = res.body.pointer("/unikey").or(res.body.pointer("/data/unikey")).and_then(Value::as_str).ok_or("未取得二维码 key")?.to_string();
    let url = format!("https://music.163.com/login?codekey={key}");
    let code = qrcode::QrCode::new(url.as_bytes()).map_err(|e| e.to_string())?;
    // quiet_zone + 固定正方形；前端白底也是正方，避免参差白边
    let raw = code
        .render::<qrcode::render::svg::Color>()
        .quiet_zone(true)
        .min_dimensions(180, 180)
        .dark_color(qrcode::render::svg::Color("#000000"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build();
    Ok(QrKey { key, svg: normalize_qr_svg(&raw, 180) })
}

/// 返回 (状态码, 登录成功时的 cookie)。800 过期 / 801 等待扫码 / 802 待确认 / 803 成功
pub async fn qr_check(key: &str) -> R<(i64, Option<String>)> {
    let c = create_client(None);
    let res = c.login_qr_check(&Query::new().param("key", key)).await.map_err(|e| e.to_string())?;
    let code = res.body.get("code").and_then(Value::as_i64).unwrap_or(res.status);
    if code == 803 {
        let cookie = res.body.get("cookie").and_then(Value::as_str).map(String::from).unwrap_or_else(|| join_set_cookie(&res.cookie));
        return Ok((code, Some(cookie)));
    }
    Ok((code, None))
}

pub async fn profile(cookie: &Option<String>) -> R<Option<Profile>> {
    if cookie.is_none() {
        return Ok(None);
    }
    let res = ok(create_client(None).user_account(&q(cookie)).await)?;
    let p = res.body.get("profile").filter(|p| !p.is_null());
    Ok(p.map(|p| Profile {
        user_id: p.get("userId").and_then(Value::as_i64).unwrap_or(0),
        nickname: p.get("nickname").and_then(Value::as_str).unwrap_or("").into(),
        avatar: p.get("avatarUrl").and_then(Value::as_str).map(String::from),
    }))
}

pub async fn daily(cookie: &Option<String>) -> R<Vec<Track>> {
    let res = ok(create_client(None).recommend_songs(&q(cookie)).await)?;
    Ok(tracks_at(&res.body["data"]["dailySongs"]))
}

pub async fn search(cookie: &Option<String>, keywords: &str, offset: u32) -> R<Vec<Track>> {
    let query = q(cookie).param("keywords", keywords).param("type", "1").param("limit", "50").param("offset", &offset.to_string());
    let res = ok(create_client(None).cloudsearch(&query).await)?;
    Ok(tracks_at(&res.body["result"]["songs"]))
}

pub async fn user_playlists(cookie: &Option<String>, uid: i64) -> R<Vec<Playlist>> {
    let query = q(cookie).param("uid", &uid.to_string()).param("limit", "1000");
    let res = ok(create_client(None).user_playlist(&query).await)?;
    let list = res.body["playlist"].as_array().cloned().unwrap_or_default();
    Ok(list
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let creator_id = p.pointer("/creator/userId").and_then(Value::as_i64).unwrap_or(0);
            let is_liked = p["specialType"].as_i64() == Some(5) || (i == 0 && creator_id == uid);
            let owned = creator_id == uid;
            Playlist {
                id: p["id"].as_i64().unwrap_or(0),
                name: p["name"].as_str().unwrap_or("").into(),
                cover: p["coverImgUrl"].as_str().map(|s| format!("{s}?param=200y200")),
                track_count: p["trackCount"].as_i64().unwrap_or(0),
                is_liked,
                owned,
                subscribed: !owned && !is_liked,
                creator_id,
            }
        })
        .collect())
}

pub async fn playlist_tracks(cookie: &Option<String>, id: i64) -> R<Vec<Track>> {
    // playlist_track_all 内部先取 trackIds 再取 song/detail；单次最多 limit 首
    let mut all = Vec::new();
    let mut offset = 0;
    loop {
        let query = q(cookie).param("id", &id.to_string()).param("limit", "500").param("offset", &offset.to_string());
        let res = ok(create_client(None).playlist_track_all(&query).await)?;
        let page = tracks_at(&res.body["songs"]);
        let n = page.len();
        all.extend(page);
        if n < 500 {
            break;
        }
        offset += 500;
    }
    Ok(all)
}

pub async fn song_url(cookie: &Option<String>, id: i64, level: &str) -> R<Option<String>> {
    let query = q(cookie).param("id", &id.to_string()).param("level", level);
    let res = ok(create_client(None).song_url_v1(&query).await)?;
    Ok(res.body.pointer("/data/0/url").and_then(Value::as_str).map(String::from))
}

/// 喜欢歌曲 id 列表（仅 id，不是详情）
pub async fn liked_ids(cookie: &Option<String>, uid: i64) -> R<Vec<i64>> {
    let query = q(cookie).param("uid", &uid.to_string());
    let res = ok(create_client(None).likelist(&query).await)?;
    Ok(res.body["ids"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_i64()).collect())
        .unwrap_or_default())
}

/// like=true 红心；false 取消
pub async fn like_track(cookie: &Option<String>, id: i64, like: bool) -> R<()> {
    let query = q(cookie)
        .param("id", &id.to_string())
        .param("like", if like { "true" } else { "false" });
    ok(create_client(None).like(&query).await)?;
    Ok(())
}

/// t=1 收藏；t=2 取消收藏
pub async fn playlist_subscribe(cookie: &Option<String>, id: i64, subscribe: bool) -> R<()> {
    let query = q(cookie)
        .param("id", &id.to_string())
        .param("t", if subscribe { "1" } else { "2" });
    ok(create_client(None).playlist_subscribe(&query).await)?;
    Ok(())
}

/// 删除自己创建的歌单（不要用于「我喜欢的音乐」）
pub async fn playlist_delete(cookie: &Option<String>, id: i64) -> R<()> {
    let query = q(cookie).param("id", &id.to_string());
    ok(create_client(None).playlist_delete(&query).await)?;
    Ok(())
}

fn parse_lrc(raw: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();
    // [mm:ss.xx]text 或 [mm:ss.xxx]
    for line in raw.lines() {
        let line = line.trim();
        if !line.starts_with('[') {
            continue;
        }
        let Some(end) = line.find(']') else { continue };
        let tag = &line[1..end];
        let text = line[end + 1..].trim().to_string();
        // 跳过 meta：[ti:] [ar:] 等
        if tag.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) == false {
            continue;
        }
        let parts: Vec<&str> = tag.split(':').collect();
        if parts.len() < 2 {
            continue;
        }
        let Ok(min) = parts[0].parse::<u64>() else { continue };
        let sec_parts: Vec<&str> = parts[1].split('.').collect();
        let Ok(sec) = sec_parts[0].parse::<u64>() else { continue };
        let frac = sec_parts.get(1).copied().unwrap_or("0");
        // 两位→百分之一秒；三位→毫秒
        let ms = if frac.len() <= 2 {
            frac.parse::<u64>().unwrap_or(0) * 10
        } else {
            frac.parse::<u64>().unwrap_or(0)
        };
        lines.push(LyricLine {
            time_ms: min * 60_000 + sec * 1000 + ms,
            text,
            translation: None,
        });
    }
    lines.sort_by_key(|l| l.time_ms);
    lines
}

fn merge_translations(mut lines: Vec<LyricLine>, tlyric: &str) -> Vec<LyricLine> {
    let tlines = parse_lrc(tlyric);
    if tlines.is_empty() {
        return lines;
    }
    // 按时间戳对齐；允许 ±50ms 误差（网易云偶发不一致）
    for line in &mut lines {
        let best = tlines.iter().min_by_key(|t| t.time_ms.abs_diff(line.time_ms));
        if let Some(t) = best {
            if t.time_ms.abs_diff(line.time_ms) <= 50 && !t.text.is_empty() {
                line.translation = Some(t.text.clone());
            }
        }
    }
    // 若几乎对不上，退化为按顺序填空行翻译
    let matched = lines.iter().filter(|l| l.translation.is_some()).count();
    if matched == 0 {
        for (line, t) in lines.iter_mut().zip(tlines.iter()) {
            if !t.text.is_empty() {
                line.translation = Some(t.text.clone());
            }
        }
    }
    lines
}

pub async fn lyric(cookie: &Option<String>, id: i64) -> R<LyricPayload> {
    let query = q(cookie).param("id", &id.to_string());
    let res = ok(create_client(None).lyric(&query).await)?;
    let raw = res.body
        .pointer("/lrc/lyric")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let translated = res
        .body
        .pointer("/tlyric/lyric")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty());
    let lines = match &translated {
        Some(t) => merge_translations(parse_lrc(&raw), t),
        None => parse_lrc(&raw),
    };
    Ok(LyricPayload {
        lines,
        raw,
        translated,
    })
}

#[cfg(test)]
mod tests {
    // 联网测试：cargo test -- --ignored
    #[tokio::test]
    #[ignore]
    async fn anonymous_search_and_qr_key() {
        let r = super::search(&None, "周杰伦", 0).await.expect("search");
        assert!(!r.is_empty());
        println!("first: {} - {:?}", r[0].title, r[0].artists);
        let k = super::qr_key().await.expect("qr");
        assert!(!k.key.is_empty() && k.svg.contains("<svg"));
        let url = super::song_url(&None, r[0].ncm_id.unwrap(), "standard").await.expect("url");
        println!("url present: {}", url.is_some());
    }
}
