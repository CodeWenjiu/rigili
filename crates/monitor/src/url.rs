pub fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    rest.split(['/', '?', '#']).next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::host;

    #[test]
    fn extracts_hosts_from_signed_stream_urls() {
        assert_eq!(
            host("https://upos-sz-mirrorcos.bilivideo.com/upgcxcode/1/2/3.m4s?deadline=1"),
            "upos-sz-mirrorcos.bilivideo.com"
        );
        assert_eq!(host("//i0.hdslb.com/bfs/emote/doge.png"), "i0.hdslb.com");
        assert_eq!(host(""), "");
    }
}
