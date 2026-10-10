//! Server-sent events, both ways: an incremental parser for an upstream
//! stream (bytes may split anywhere), and the writer for the CLI's side.

use serde_json::Value;

#[derive(Default)]
pub struct Parser {
    buf: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}

impl Parser {
    /// Feed bytes; get every event they complete, as (event name, data).
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<(Option<String>, String)> {
        self.buf.extend_from_slice(bytes);
        let mut out = vec![];
        let mut start = 0;
        while let Some(i) = self.buf[start..].iter().position(|b| *b == b'\n') {
            let end = start + i;
            let mut line = &self.buf[start..end];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
            let line = String::from_utf8_lossy(line).into_owned();
            start = end + 1;
            if line.is_empty() {
                if !self.data.is_empty() || self.event.is_some() {
                    out.push((self.event.take(), self.data.join("\n")));
                    self.data.clear();
                }
                continue;
            }
            if line.starts_with(':') {
                continue;
            }
            let (field, value) = match line.find(':') {
                Some(i) => (&line[..i], line[i + 1..].strip_prefix(' ').unwrap_or(&line[i + 1..])),
                None => (line.as_str(), ""),
            };
            match field {
                "event" => self.event = Some(value.to_string()),
                "data" => self.data.push(value.to_string()),
                _ => {}
            }
        }
        self.buf.drain(..start);
        out
    }
}

/// One event as the Anthropic API writes it.
pub fn event(name: &str, data: &Value) -> String {
    format!("event: {name}\ndata: {data}\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_split_anywhere_are_put_back_together() {
        let stream = "data: {\"a\":1}\n\n: comment\nevent: ping\ndata: x\r\ndata: y\n\ndata: [DONE]\n\n";
        for cut in 0..stream.len() {
            let mut p = Parser::default();
            let mut got = p.feed(&stream.as_bytes()[..cut]);
            got.extend(p.feed(&stream.as_bytes()[cut..]));
            assert_eq!(got, vec![(None, "{\"a\":1}".to_string()), (Some("ping".into()), "x\ny".into()), (None, "[DONE]".into())], "cut at {cut}");
        }
    }
}
