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

/// 网易云 CDN / 试听地址常返回 http://。Tauri 正式包 webview 是 https 源，
/// http 资源会被当成混合内容拦掉（封面蓝 ?、第一次播放失败）。统一升 https。
pub fn ncm_https_url(raw: &str) -> String {
    let s = raw.trim();
    if let Some(rest) = s.strip_prefix("http://") {
        format!("https://{rest}")
    } else if let Some(rest) = s.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        s.to_string()
    }
}

/// 封面图：升 https 并规范 ?param= 尺寸。
pub fn ncm_img_url(raw: &str, param: &str) -> String {
    let s = ncm_https_url(raw);
    let base = s.split_once('?').map(|(b, _)| b).unwrap_or(s.as_str());
    format!("{base}?param={param}")
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
        cover: al
            .and_then(|a| a.get("picUrl"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| ncm_img_url(s, "200y200")),
        path: None,
        ncm_id: Some(id),
        ..Default::default()
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
        avatar: p.get("avatarUrl").and_then(Value::as_str).filter(|s| !s.is_empty()).map(|s| ncm_img_url(s, "100y100")),
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
                cover: p["coverImgUrl"].as_str().filter(|s| !s.is_empty()).map(|s| ncm_img_url(s, "200y200")),
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
        let query = q(cookie).param("id", &id.to_string()).param("limit", "500").param("offset", &offset.to_string()).param("s", "0");
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

/// 音质 level → 旧版 song/url 的 br（v1 挂掉时的兜底）。
fn level_to_br(level: &str) -> &'static str {
    match level {
        "standard" => "128000",
        "higher" => "192000",
        "exhigh" => "320000",
        // lossless / hires / sky / jymaster 等：尽量要最高
        _ => "999000",
    }
}

fn url_from_data0(body: &Value) -> Option<String> {
    body.pointer("/data/0/url")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(ncm_https_url)
}

/// 取播放直链。优先 song_url_v1；网易云/加密路径偶发对 v1 返回怪 code（被映射成
/// Invalid parameter）时，依次回退 download/v1（v1_302）与旧版 song/url。
/// 错误信息只含 id/level/端点结果，不含 cookie。
pub async fn song_url(cookie: &Option<String>, id: i64, level: &str) -> R<Option<String>> {
    let client = create_client(None);
    let mut stages: Vec<String> = Vec::new();

    // 1) player/url/v1（按 level，可到 hires）
    {
        let query = q(cookie).param("id", &id.to_string()).param("level", level);
        match client.song_url_v1(&query).await {
            Ok(r) => {
                if r.status == 200 {
                    if let Some(url) = url_from_data0(&r.body) {
                        return Ok(Some(url));
                    }
                    stages.push(format!("v1:ok_empty(code={:?})", r.body.get("code")));
                } else {
                    stages.push(format!("v1:status={}", r.status));
                }
            }
            Err(e) => stages.push(format!("v1:{e}")),
        }
    }

    // 2) download/url/v1 → redirectUrl（ncm-api-rs 的 song_url_v1_302）
    {
        let query = q(cookie).param("id", &id.to_string()).param("level", level);
        match client.song_url_v1_302(&query).await {
            Ok(r) => {
                if let Some(url) = r
                    .body
                    .get("redirectUrl")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    return Ok(Some(ncm_https_url(url)));
                }
                if let Some(url) = url_from_data0(&r.body) {
                    return Ok(Some(url));
                }
                stages.push(format!("v1_302:ok_empty(status={})", r.status));
            }
            Err(e) => stages.push(format!("v1_302:{e}")),
        }
    }

    // 3) 旧版 player/url（br）
    {
        let br = level_to_br(level);
        let query = q(cookie).param("id", &id.to_string()).param("br", br);
        match client.song_url(&query).await {
            Ok(r) => {
                if r.status == 200 || r.status == 302 {
                    if let Some(url) = url_from_data0(&r.body) {
                        return Ok(Some(url));
                    }
                    // 无版权 / VIP：各端都空，返回 None（前端跳过）
                    stages.push(format!("v0:ok_empty(code={:?},br={br})", r.body.get("code")));
                    return Ok(None);
                }
                stages.push(format!("v0:status={}", r.status));
            }
            Err(e) => stages.push(format!("v0:{e}")),
        }
    }

    Err(format!(
        "取直链失败 id={id} level={level} | {}",
        stages.join(" → ")
    ))
}

fn json_i64(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_u64().and_then(|n| i64::try_from(n).ok()))
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// 从 playlist/detail 取出 (id, 收藏时间 at)。优先 trackIds；CDN 偶发只给 tracks。
fn liked_track_entries(playlist: &Value) -> Vec<(i64, i64)> {
    let from_ids = playlist["trackIds"].as_array().map(|a| {
        a.iter()
            .filter_map(|t| Some((json_i64(&t["id"])?, json_i64(&t["at"]).unwrap_or(0))))
            .collect::<Vec<_>>()
    });
    if let Some(entries) = from_ids {
        if !entries.is_empty() {
            return entries;
        }
    }
    playlist["tracks"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| json_i64(&t["id"]).map(|id| (id, 0)))
                .collect()
        })
        .unwrap_or_default()
}

/// 「我喜欢的音乐」：顺序以歌单自身 trackIds 为准（`at` 为收藏时间，按其倒序）；
/// likelist 的 id 不按收藏时间排序，只用来补齐 CDN 未更新的新收藏（置顶）并剔除已取消的。
pub async fn liked_tracks(cookie: &Option<String>, uid: i64, playlist_id: i64) -> R<Vec<Track>> {
    let query = q(cookie).param("id", &playlist_id.to_string()).param("s", "0");
    let res = ok(create_client(None).playlist_detail(&query).await)?;
    let mut entries = liked_track_entries(&res.body["playlist"]);
    if !entries.is_empty() && entries.iter().all(|e| e.1 > 0) {
        entries.sort_by(|x, y| y.1.cmp(&x.1)); // stable; 收藏时间倒序
    }
    let mut order: Vec<i64> = entries.into_iter().map(|e| e.0).collect();
    if uid > 0 {
        match liked_ids(cookie, uid).await {
            Ok(likes) if !likes.is_empty() => {
                let like_set: std::collections::HashSet<i64> = likes.iter().copied().collect();
                let have: std::collections::HashSet<i64> = order.iter().copied().collect();
                let mut fresh: Vec<i64> = likes.iter().copied().filter(|id| !have.contains(id)).collect();
                order.retain(|id| like_set.contains(id));
                fresh.extend(order);
                order = fresh;
            }
            Ok(_) => { /* likelist 确认无新红心，保留 trackIds（可能本来就是空歌单） */ }
            Err(e) => {
                // trackIds 也是空时不能当成「没有喜欢的歌」成功返回，否则会被缓存
                if order.is_empty() {
                    return Err(e);
                }
            }
        }
    } else if order.is_empty() {
        return Err("无法获取「我喜欢的音乐」：未取得用户信息".into());
    }
    let mut all = Vec::with_capacity(order.len());
    for chunk in order.chunks(500) {
        let joined = chunk.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        let query = q(cookie).param("ids", &joined);
        let res = ok(create_client(None).song_detail(&query).await)?;
        let mut page = tracks_at(&res.body["songs"]);
        let pos: std::collections::HashMap<i64, usize> = chunk.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        page.sort_by_key(|t| t.ncm_id.and_then(|id| pos.get(&id).copied()).unwrap_or(usize::MAX));
        all.extend(page);
    }
    Ok(all)
}

/// 喜欢歌曲的ID列表
pub async fn liked_ids(cookie: &Option<String>, uid: i64) -> R<Vec<i64>> {
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let query = q(cookie).param("uid", &uid.to_string()).param("timestamp", &ts.to_string());
    let res = ok(create_client(None).likelist(&query).await)?;
    Ok(res.body["ids"]
        .as_array()
        .map(|a| a.iter().filter_map(json_i64).collect())
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

/// 新建网易云歌单，返回新歌单 id
pub async fn playlist_create(cookie: &Option<String>, name: &str) -> R<i64> {
    let name = name.trim();
    if name.is_empty() {
        return Err("请输入歌单名称".into());
    }
    let query = q(cookie).param("name", name);
    let res = ok(create_client(None).playlist_create(&query).await)?;
    res.body
        .pointer("/playlist/id")
        .or_else(|| res.body.get("id"))
        .and_then(Value::as_i64)
        .ok_or_else(|| "已创建，但未返回歌单 id".into())
}

/// 删除自己创建的歌单（不要用于「我喜欢的音乐」）
pub async fn playlist_delete(cookie: &Option<String>, id: i64) -> R<()> {
    let query = q(cookie).param("id", &id.to_string());
    ok(create_client(None).playlist_delete(&query).await)?;
    Ok(())
}

/// 解析 LRC 时间标签：`mm:ss`、`mm:ss.x(x)(x)`，也接受 `hh:mm:ss`。
/// 分数位按毫秒左对齐：`.5` / `.50` / `.500` 都是 500ms。
pub fn parse_lrc_time(tag: &str) -> Option<u64> {
    let tag = tag.trim();
    if tag.is_empty() || !tag.as_bytes().first()?.is_ascii_digit() {
        return None;
    }
    let (hms, frac) = match tag.split_once('.') {
        Some((a, b)) => (a, b),
        None => (tag, ""),
    };
    if frac.chars().any(|c| !c.is_ascii_digit()) {
        return None;
    }
    let ms = if frac.is_empty() {
        0
    } else {
        let t: String = frac.chars().take(3).collect();
        let n: u64 = t.parse().ok()?;
        n * 10u64.pow((3 - t.len()) as u32)
    };
    let mut parts = Vec::new();
    for p in hms.split(':') {
        parts.push(p.parse::<u64>().ok()?);
    }
    let (h, m, s) = match parts.as_slice() {
        [m, s] => (0, *m, *s),
        [h, m, s] => (*h, *m, *s),
        _ => return None,
    };
    if s >= 60 {
        return None;
    }
    Some(h * 3_600_000 + m * 60_000 + s * 1000 + ms)
}

/// 去掉逐字歌词 `<mm:ss.xx>` 时间戳，只保留文本。
fn strip_enhanced_lrc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        if let Some(rel) = rest[i + 1..].find('>') {
            let inner = &rest[i + 1..i + 1 + rel];
            if parse_lrc_time(inner).is_some() {
                rest = &rest[i + 2 + rel..];
                continue;
            }
        }
        out.push('<');
        rest = &rest[i + 1..];
    }
    out.push_str(rest);
    out.trim().to_string()
}

pub fn parse_lrc(raw: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();
    // [mm:ss.xx]text；同一行多个时间戳（副歌复用）各生成一行
    for line in raw.lines() {
        let mut rest = line.trim();
        if !rest.starts_with('[') {
            continue;
        }
        let mut times = Vec::new();
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            let tag = &rest[1..end];
            if let Some(ms) = parse_lrc_time(tag) {
                times.push(ms);
                rest = rest[end + 1..].trim_start();
            } else {
                break;
            }
        }
        if times.is_empty() {
            continue;
        }
        let text = strip_enhanced_lrc(rest.trim());
        for time_ms in times {
            lines.push(LyricLine {
                time_ms,
                text: text.clone(),
                translation: None,
            });
        }
    }
    lines.sort_by_key(|l| l.time_ms);
    lines
}

/// 把译文 LRC 按时间戳（±50ms）对齐到原文；对不上则按顺序填。
pub fn merge_translations(mut lines: Vec<LyricLine>, tlyric: &str) -> Vec<LyricLine> {
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
    #[test]
    fn ncm_https_url_upgrades_http() {
        assert_eq!(
            super::ncm_https_url("http://m7.music.126.net/x.flac?vuutv=1"),
            "https://m7.music.126.net/x.flac?vuutv=1"
        );
        assert_eq!(
            super::ncm_https_url("//m7.music.126.net/x.mp3"),
            "https://m7.music.126.net/x.mp3"
        );
    }

    #[test]
    fn ncm_img_url_upgrades_http_and_sets_param() {
        assert_eq!(
            super::ncm_img_url(
                "http://p3.music.126.net/abc==/1.jpg",
                "200y200"
            ),
            "https://p3.music.126.net/abc==/1.jpg?param=200y200"
        );
        assert_eq!(
            super::ncm_img_url(
                "https://p3.music.126.net/abc==/1.jpg?param=140y140",
                "200y200"
            ),
            "https://p3.music.126.net/abc==/1.jpg?param=200y200"
        );
        assert_eq!(
            super::ncm_img_url("//p1.music.126.net/x.jpg", "500y500"),
            "https://p1.music.126.net/x.jpg?param=500y500"
        );
    }

    // 联网测试：cargo test -- --ignored
    #[test]
    fn level_to_br_mapping() {
        assert_eq!(super::level_to_br("standard"), "128000");
        assert_eq!(super::level_to_br("higher"), "192000");
        assert_eq!(super::level_to_br("exhigh"), "320000");
        assert_eq!(super::level_to_br("hires"), "999000");
        assert_eq!(super::level_to_br("lossless"), "999000");
    }

    #[test]
    fn json_i64_parses_number_and_string() {
        use serde_json::json;
        assert_eq!(super::json_i64(&json!(12345)), Some(12345));
        assert_eq!(super::json_i64(&json!("67890")), Some(67890));
        assert_eq!(super::json_i64(&json!(" 42 ")), Some(42));
        assert_eq!(super::json_i64(&json!(null)), None);
    }

    #[test]
    fn liked_entries_prefer_track_ids() {
        use serde_json::json;
        let playlist = json!({
            "trackIds": [
                {"id": 1, "at": 100},
                {"id": "2", "at": "200"}
            ],
            "tracks": [{"id": 99}]
        });
        assert_eq!(super::liked_track_entries(&playlist), vec![(1, 100), (2, 200)]);
    }

    #[test]
    fn liked_entries_fall_back_to_tracks_when_ids_missing() {
        use serde_json::json;
        assert_eq!(
            super::liked_track_entries(&json!({"tracks": [{"id": 7}, {"id": "8"}]})),
            vec![(7, 0), (8, 0)]
        );
        assert!(super::liked_track_entries(&json!({})).is_empty());
    }

    #[test]
    fn parse_lrc_time_fractions_and_hours() {
        assert_eq!(super::parse_lrc_time("00:01.5"), Some(1500));
        assert_eq!(super::parse_lrc_time("00:01.50"), Some(1500));
        assert_eq!(super::parse_lrc_time("00:01.500"), Some(1500));
        assert_eq!(super::parse_lrc_time("01:02.03"), Some(62_030));
        assert_eq!(super::parse_lrc_time("1:02:03.000"), Some(3_723_000));
        assert_eq!(super::parse_lrc_time("ti:title"), None);
        assert_eq!(super::parse_lrc_time("00:61.00"), None);
    }

    #[test]
    fn parse_lrc_multi_timestamp_and_enhanced() {
        let lines = super::parse_lrc("[00:01.00][00:10.00]Hello\n[00:02.00]Let <00:02.20>go");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].time_ms, 1000);
        assert_eq!(lines[0].text, "Hello");
        assert_eq!(lines[1].time_ms, 2000);
        assert_eq!(lines[1].text, "Let go");
        assert_eq!(lines[2].time_ms, 10_000);
        assert_eq!(lines[2].text, "Hello");
    }

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
