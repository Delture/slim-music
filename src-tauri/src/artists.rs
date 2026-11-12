//! 艺人名称拆分，以及含分隔符的官方名例外名单。
//!
//! 本地扫描、Android MediaStore 与 `local_enrich`（含网易云曲目补索引）都走这里，
//! 保证各音源拆分规则一致。例外名单只收录能核验的官方艺人/乐队名，按完整名称精确匹配，
//! 不是松散子串——以免挡住「A、B」这种正常合作拆分。

/// 分隔符：斜线、各类分号/逗号、顿号、NUL（部分标签用 `\0` 拼多名）。
const DELIMS: [char; 7] = ['/', ';', '；', '、', ',', '，', '\0'];

/// 官方艺人名本身含有上列分隔符，拆分时必须保持完整。
///
/// 匹配规则：完整官方名精确命中（ASCII 大小写不敏感，保留原文大小写），
/// 且两侧为字符串边界、空白或分隔符，避免「Earth, Wind & Firestorm」这类子串误伤。
/// 查找时按名称从长到短，以便「Crosby, Stills, Nash & Young」优先于较短条目。
///
/// 来源见各条目注释。名单刻意精简，不收录存疑或无法核验的名字。
const ARTIST_EXCEPTIONS: &[&str] = &[
    // —— 含「、」（日语/中文顿号）——
    // 日本乐队。日文维基、MusicBrainz 032f2f38-8d11-43e2-ad0e-949eb1bb9f9a、网易云艺人 id 12203334
    "神様、僕は気づいてしまった",
    // 上一条在网易云的官方中文译名（artist.transNames）
    "神啊、我已经察觉到了",
    // 以下均为 MusicBrainz 收录的 Group，官方名本身含顿号（非多人合作串）
    "さっきの女の子、",           // MB 8769d3d1-a069-482e-9f99-b6896c772e69，日本
    "午前四時、朝焼けにツキ",     // MB 578a146e-6cbe-4c84-bd87-a0588f39b82c，日本
    "7秒後、君が好きになる。",    // MB e1912bed-ff04-4f86-95c0-e5b6bbc499ed，东京
    "ワタシは雨、",               // MB 5db6daaf-8cab-4c27-8357-bc082129b325，日本
    "東京、君がいない街",         // MB 4804ae20-6116-4f92-a061-658f8ec48116，日本
    "おどろき、等々力、六本木!!", // MB c712623a-854d-4815-b1f7-1a81018a35d7，日本
    "終生、顔を成し。",           // MB fe6edebe-4242-4660-8140-ab307f0a2352，日本
    "宙の時刻表、最終便",         // MB 5630cf09-6f30-43a9-bec7-9b0193ef0ae6，日本
    // —— 含 "," 或 "/"（欧美官方名）——
    "Crosby, Stills, Nash & Young", // MB 46a782ea-4308-476b-abd1-a91b197f3037；Wikipedia
    "Hamilton, Joe Frank & Reynolds", // MB a2fcf471-0fca-44b1-b074-ec45ad59e205；Wikipedia
    "Earth, Wind & Fire",           // MB 535afeda-2538-435d-9dd1-5e10be586774；Wikipedia
    "Blood, Sweat & Tears",         // MB f81d592c-993f-4693-b53a-57db5a6257b9；Wikipedia
    "Crosby, Stills & Nash",        // MB cfafad90-b736-4c74-8ded-f3f4d7b2c792；Wikipedia
    "Emerson, Lake & Palmer",       // MB ad996aef-cc1c-42ac-af5c-619c370f4b8a；Wikipedia
    "Emerson, Lake & Powell",       // MB 9a78e16a-2e3b-44b5-8aee-4546f4f4177f；Wikipedia
    "Peter, Paul and Mary",         // MB 3d621c94-2356-4532-ad52-79e0ef73e20c；Wikipedia
    "10,000 Maniacs", // MB b9a06530-1241-4162-836f-7b8e79deaa58；Wikipedia（千分逗号）
    "AC/DC",          // MB 66c662b6-6e2f-4930-8610-912e24c63ed1；Wikipedia
    "K/DA",           // MB 35f3e736-e27b-4a26-892c-c00179d163d5；Riot 虚拟组合
    "TR/ST",          // MB b8e3d1ae-5983-4af1-b226-aa009b294111；加拿大电子项目官方拼写
];

fn is_boundary(c: char) -> bool {
    c.is_whitespace() || DELIMS.contains(&c)
}

/// 在 `hay` 中找 `needle`（ASCII 大小写不敏感），且两侧为边界。返回字节下标。
fn find_exception(hay: &str, needle: &str) -> Option<usize> {
    let nlen = needle.len();
    if nlen == 0 || nlen > hay.len() {
        return None;
    }
    for (i, _) in hay.char_indices() {
        let end = i + nlen;
        if end > hay.len() {
            break;
        }
        if !hay.is_char_boundary(end) {
            continue;
        }
        if !hay[i..end].eq_ignore_ascii_case(needle) {
            continue;
        }
        let left_ok = i == 0 || hay[..i].chars().next_back().is_some_and(is_boundary);
        let right_ok = end == hay.len() || hay[end..].chars().next().is_some_and(is_boundary);
        if left_ok && right_ok {
            return Some(i);
        }
    }
    None
}

/// 把官方例外名替换成不含分隔符的占位符，拆分后再还原（保留原文大小写）。
fn protect_exceptions(s: &str) -> (String, Vec<String>) {
    let mut names: Vec<&str> = ARTIST_EXCEPTIONS.to_vec();
    names.sort_by_key(|n| std::cmp::Reverse(n.len()));

    let mut out = s.to_string();
    let mut captured: Vec<String> = Vec::new();
    loop {
        let mut hit: Option<(usize, usize, String)> = None;
        for name in &names {
            if let Some(i) = find_exception(&out, name) {
                let end = i + name.len();
                // 同长度时取更靠前的；已按长度降序，先命中的就是当前最长
                hit = Some((i, end, out[i..end].to_string()));
                break;
            }
        }
        let Some((i, end, orig)) = hit else { break };
        let token = format!("\u{E000}{}\u{E001}", captured.len());
        out.replace_range(i..end, &token);
        captured.push(orig);
    }
    (out, captured)
}

fn restore_exceptions(piece: &str, captured: &[String]) -> String {
    let mut s = piece.to_string();
    for (i, orig) in captured.iter().enumerate().rev() {
        let token = format!("\u{E000}{i}\u{E001}");
        s = s.replace(&token, orig);
    }
    s
}

/// 将标签/元数据里的艺人字符串拆成多名。例外名单中的官方名保持完整。
pub fn split_artists(v: &str) -> Vec<String> {
    let mut s = v.to_string();
    for sep in [" feat. ", " ft. ", " Feat. ", " FEAT. ", " featuring "] {
        s = s.replace(sep, "/");
    }
    let (protected, captured) = protect_exceptions(&s);
    protected
        .split(DELIMS)
        .map(|x| restore_exceptions(x.trim(), &captured))
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_normal_ideographic_comma() {
        assert_eq!(split_artists("蔡依林、周杰伦"), vec!["蔡依林", "周杰伦"]);
        assert_eq!(split_artists("A / B、C feat. D"), vec!["A", "B", "C", "D"]);
        assert_eq!(
            split_artists("蔡依林，周杰伦, X"),
            vec!["蔡依林", "周杰伦", "X"]
        );
        assert_eq!(
            split_artists("Simon & Garfunkel"),
            vec!["Simon & Garfunkel"]
        );
    }

    #[test]
    fn exception_stays_whole() {
        assert_eq!(
            split_artists("神様、僕は気づいてしまった"),
            vec!["神様、僕は気づいてしまった"]
        );
        assert_eq!(split_artists("AC/DC"), vec!["AC/DC"]);
        assert_eq!(
            split_artists("Earth, Wind & Fire"),
            vec!["Earth, Wind & Fire"]
        );
        assert_eq!(split_artists("10,000 Maniacs"), vec!["10,000 Maniacs"]);
        // 保留原文大小写
        assert_eq!(split_artists("ac/dc"), vec!["ac/dc"]);
    }

    #[test]
    fn exception_mixed_with_another_artist() {
        assert_eq!(
            split_artists("神様、僕は気づいてしまった、周杰伦"),
            vec!["神様、僕は気づいてしまった", "周杰伦"]
        );
        assert_eq!(split_artists("AC/DC / 周杰伦"), vec!["AC/DC", "周杰伦"]);
        assert_eq!(
            split_artists("Earth, Wind & Fire feat. 蔡依林"),
            vec!["Earth, Wind & Fire", "蔡依林"]
        );
        assert_eq!(
            split_artists("Crosby, Stills, Nash & Young、Bob Dylan"),
            vec!["Crosby, Stills, Nash & Young", "Bob Dylan"]
        );
    }

    #[test]
    fn exception_is_not_loose_substring() {
        // 不因「AC/DC」而挡住「AC、DC」或「AC/DC」之外的残片
        assert_eq!(split_artists("AC、DC"), vec!["AC", "DC"]);
        // 官方名后面紧跟字母，不是边界，不保护
        assert_eq!(
            split_artists("Earth, Wind & Firestorm"),
            vec!["Earth", "Wind & Firestorm"]
        );
        // 完整名 + 分隔符才保护
        assert_eq!(
            split_artists("Earth, Wind & Fire, 蔡依林"),
            vec!["Earth, Wind & Fire", "蔡依林"]
        );
    }
}
