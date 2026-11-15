use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DanmakuMode {
    #[default]
    Scroll,
    Top,
    Bottom,
    Reverse,
    Advanced,
    Code,
}

impl From<u8> for DanmakuMode {
    fn from(mode: u8) -> Self {
        match mode {
            4 => Self::Bottom,
            5 => Self::Top,
            6 => Self::Reverse,
            7 => Self::Advanced,
            8 => Self::Code,
            _ => Self::Scroll,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Danmaku {
    pub time: f64,
    pub mode: DanmakuMode,
    pub font_size: u8,
    pub color: u32,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Error)]
#[error("弹幕分段数据格式异常")]
pub struct DanmakuError;

pub(crate) fn parse_segment(bytes: &[u8]) -> Result<Vec<Danmaku>, DanmakuError> {
    let mut reader = Reader::new(bytes);
    let mut items = Vec::new();
    while !reader.done() {
        let (field, wire) = reader.tag()?;
        if field == 1 && wire == 2 {
            let elem = reader.length_delimited()?;
            if let Some(item) = parse_elem(elem)? {
                items.push(item);
            }
        } else {
            reader.skip(wire)?;
        }
    }
    Ok(items)
}

fn parse_elem(bytes: &[u8]) -> Result<Option<Danmaku>, DanmakuError> {
    let mut reader = Reader::new(bytes);
    let mut progress = None;
    let mut mode = 1;
    let mut font_size = 25;
    let mut color = 0x00ff_ffff;
    let mut text = String::new();

    while !reader.done() {
        let (field, wire) = reader.tag()?;
        match (field, wire) {
            (2, 0) => progress = Some(reader.varint()?),
            (3, 0) => mode = reader.varint()? as u8,
            (4, 0) => font_size = reader.varint()?.clamp(1, u8::MAX as u64) as u8,
            (5, 0) => color = (reader.varint()? & 0x00ff_ffff) as u32,
            (7, 2) => {
                text = std::str::from_utf8(reader.length_delimited()?)
                    .map_err(|_| DanmakuError)?
                    .replace(['\n', '\r'], " ");
            }
            _ => reader.skip(wire)?,
        }
    }

    let Some(progress) = progress else {
        return Ok(None);
    };
    if text.is_empty() {
        return Ok(None);
    }

    Ok(Some(Danmaku {
        time: progress as f64 / 1000.0,
        mode: DanmakuMode::from(mode),
        font_size,
        color,
        text,
    }))
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn done(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn varint(&mut self) -> Result<u64, DanmakuError> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *self.bytes.get(self.pos).ok_or(DanmakuError)?;
            self.pos += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(DanmakuError)
    }

    fn tag(&mut self) -> Result<(u32, u8), DanmakuError> {
        let tag = self.varint()?;
        let field = (tag >> 3) as u32;
        if field == 0 {
            return Err(DanmakuError);
        }
        Ok((field, (tag & 7) as u8))
    }

    fn length_delimited(&mut self) -> Result<&'a [u8], DanmakuError> {
        let len = self.varint()? as usize;
        let end = self.pos.checked_add(len).ok_or(DanmakuError)?;
        let slice = self.bytes.get(self.pos..end).ok_or(DanmakuError)?;
        self.pos = end;
        Ok(slice)
    }

    fn skip(&mut self, wire: u8) -> Result<(), DanmakuError> {
        match wire {
            0 => {
                self.varint()?;
            }
            1 => self.advance(8)?,
            2 => {
                self.length_delimited()?;
            }
            5 => self.advance(4)?,
            _ => return Err(DanmakuError),
        }
        Ok(())
    }

    fn advance(&mut self, len: usize) -> Result<(), DanmakuError> {
        self.pos = self.pos.checked_add(len).ok_or(DanmakuError)?;
        if self.pos > self.bytes.len() {
            return Err(DanmakuError);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Danmaku, DanmakuMode, parse_segment};

    fn varint(mut value: u64, out: &mut Vec<u8>) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    fn field_varint(field: u32, value: u64, out: &mut Vec<u8>) {
        varint(u64::from(field) << 3, out);
        varint(value, out);
    }

    fn field_bytes(field: u32, bytes: &[u8], out: &mut Vec<u8>) {
        varint((u64::from(field) << 3) | 2, out);
        varint(bytes.len() as u64, out);
        out.extend_from_slice(bytes);
    }

    fn elem(progress: u64, mode: u64, size: u64, color: u64, text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        field_varint(2, progress, &mut out);
        field_varint(3, mode, &mut out);
        field_varint(4, size, &mut out);
        field_varint(5, color, &mut out);
        field_bytes(7, text.as_bytes(), &mut out);
        out
    }

    #[test]
    fn parses_danmaku_segments() {
        let first = elem(12_500, 1, 25, 0xff0000, "前方高能");
        let second = elem(3_000, 5, 36, 0xffffff, "顶部弹幕");
        let mut segment = Vec::new();
        field_bytes(1, &first, &mut segment);
        field_bytes(1, &second, &mut segment);

        let items = parse_segment(&segment).expect("segment");
        assert_eq!(
            items,
            [
                Danmaku {
                    time: 12.5,
                    mode: DanmakuMode::Scroll,
                    font_size: 25,
                    color: 0xff0000,
                    text: "前方高能".into(),
                },
                Danmaku {
                    time: 3.0,
                    mode: DanmakuMode::Top,
                    font_size: 36,
                    color: 0xffffff,
                    text: "顶部弹幕".into(),
                },
            ]
        );
    }

    #[test]
    fn skips_unknown_fields_and_empty_runs() {
        let mut first = elem(1_000, 4, 18, 0x00ff00, "底部");
        field_varint(99, 42, &mut first);
        field_bytes(98, b"ignored", &mut first);
        let mut segment = Vec::new();
        field_varint(15, 7, &mut segment);
        field_bytes(1, &first, &mut segment);
        field_bytes(1, &elem(2_000, 1, 25, 0xffffff, ""), &mut segment);

        let items = parse_segment(&segment).expect("segment");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].mode, DanmakuMode::Bottom);
        assert_eq!(items[0].color, 0x00ff00);
    }

    #[test]
    fn rejects_truncated_segments() {
        let mut segment = Vec::new();
        field_bytes(1, &elem(1_000, 1, 25, 0xffffff, "abc"), &mut segment);
        segment.pop();
        assert!(parse_segment(&segment).is_err());
    }

    #[test]
    fn parses_a_real_segment_fixture() {
        let bytes = include_bytes!("../../fixtures/danmaku_seg.bin");
        let items = parse_segment(bytes).expect("fixture");

        assert!(!items.is_empty());
        for item in &items {
            assert!(!item.text.is_empty());
            assert!(
                (0.0..360.0).contains(&item.time),
                "{} out of range",
                item.time
            );
            assert!(item.color <= 0x00ff_ffff);
            assert!(item.font_size > 0);
        }
    }

    #[test]
    fn maps_danmaku_modes() {
        assert_eq!(DanmakuMode::from(1), DanmakuMode::Scroll);
        assert_eq!(DanmakuMode::from(4), DanmakuMode::Bottom);
        assert_eq!(DanmakuMode::from(5), DanmakuMode::Top);
        assert_eq!(DanmakuMode::from(6), DanmakuMode::Reverse);
        assert_eq!(DanmakuMode::from(7), DanmakuMode::Advanced);
        assert_eq!(DanmakuMode::from(8), DanmakuMode::Code);
    }
}
