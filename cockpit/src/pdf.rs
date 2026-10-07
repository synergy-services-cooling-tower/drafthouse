//! A minimal PDF 1.7 writer (issue #85) - written here because `cockpit/Cargo.toml` is fenced and
//! the export must not add a dependency.
//!
//! What it emits: a catalog, a page tree, one uncompressed content stream per page, three standard
//! fonts (Helvetica, Helvetica-Bold, Courier - no embedding, every reader ships them) and an info
//! dictionary. What the content streams carry:
//!
//! - **text** as real text operators (`BT … Tj ET`), so the sheet's words and numbers are
//!   selectable and extractable, never a picture of a sentence;
//! - **paths** as real path operators (`m`/`l`/`re`/`c`), so every chart is vector.
//!
//! Text is written in the fonts' WinAnsi encoding: the subset of Latin-1 the sheet actually uses
//! (`° ³ · × – — …` and the curly quotes) maps to its WinAnsi byte, ASCII passes through, and the
//! glyphs WinAnsi has no code for are substituted by their plain ASCII spelling -
//! `≥` becomes `>=`, `≤` becomes `<=`, the minus sign `−` becomes `-`. Nothing else changes; anything unmapped becomes `?`, so a
//! substitution can never silently vanish. [`extract_text`] (used by the module's tests and the
//! report's evidence) decodes exactly the operators this module writes, byte for byte.
//!
//! Coordinates are given **y down from the top-left** of an A4 page (the way the screen's own
//! painters think); the writer flips them into PDF's y-up space. No compression, no encryption, no
//! `/ID`: the same input always serialises to the same bytes, which is what the state-hash and the
//! re-runs in the evidence rely on.

/// A4 width in points (210 mm).
pub const A4_W: f64 = 595.28;
/// A4 height in points (297 mm).
pub const A4_H: f64 = 841.89;

/// The three standard fonts the sheet uses. `/F1` `/F2` `/F3` in the content streams.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Font {
    /// Helvetica.
    Sans,
    /// Helvetica-Bold.
    Bold,
    /// Courier (fixed 600/1000 em).
    Mono,
}

impl Font {
    fn resource(self) -> &'static str {
        match self {
            Font::Sans => "F1",
            Font::Bold => "F2",
            Font::Mono => "F3",
        }
    }
}

/// One page under construction: the ops written so far, as PDF bytes.
struct Page {
    ops: Vec<u8>,
}

/// The writer. Build pages with the drawing calls, then [`Pdf::finish`] for the file bytes.
pub struct Pdf {
    pages: Vec<Page>,
    title: String,
}

impl Pdf {
    /// A document whose info dictionary carries `title`.
    pub fn new(title: &str) -> Self {
        Self {
            pages: vec![Page { ops: Vec::new() }],
            title: title.to_string(),
        }
    }

    /// Close the current page and start the next one.
    pub fn page(&mut self) {
        self.pages.push(Page { ops: Vec::new() });
    }

    /// How many pages the document has so far.
    pub fn pages(&self) -> usize {
        self.pages.len()
    }

    fn op(&mut self, bytes: &[u8]) {
        self.pages
            .last_mut()
            .expect("the writer always has a page")
            .ops
            .extend_from_slice(bytes);
    }

    fn color_prefix(&mut self, rgb: [u8; 3], stroke: bool) {
        let (r, g, b) = rgb01(rgb);
        let op = if stroke { "RG" } else { "rg" };
        self.op(format!("{r} {g} {b} {op}\n").as_bytes());
    }

    /// One line of text with its baseline at `(x, y)` (y down), whole string in one `Tj`.
    pub fn text(&mut self, x: f64, y: f64, size: f64, font: Font, rgb: [u8; 3], s: &str) {
        if s.is_empty() {
            return;
        }
        self.color_prefix(rgb, false);
        let head = format!(
            "BT /{} {} Tf 1 0 0 1 {} {} Tm (",
            font.resource(),
            pt(size),
            pt(x),
            pt(A4_H - y)
        );
        self.op(head.as_bytes());
        let encoded = encode(s);
        self.write_literal(&encoded);
        self.op(b") Tj ET\n");
    }

    /// Text rotated counter-clockwise by `deg` degrees about `(x, y)` (y down).
    #[allow(clippy::too_many_arguments)]
    pub fn text_rot(
        &mut self,
        x: f64,
        y: f64,
        size: f64,
        font: Font,
        rgb: [u8; 3],
        deg: f64,
        s: &str,
    ) {
        let a = deg.to_radians();
        let (cos, sin) = (a.cos(), a.sin());
        self.color_prefix(rgb, false);
        let head = format!(
            "BT /{} {} Tf {} {} {} {} {} {} Tm (",
            font.resource(),
            pt(size),
            pt(cos),
            pt(sin),
            pt(-sin),
            pt(cos),
            pt(x),
            pt(A4_H - y)
        );
        self.op(head.as_bytes());
        let encoded = encode(s);
        self.write_literal(&encoded);
        self.op(b") Tj ET\n");
    }

    /// Text at `x`, centred on `center_x`.
    pub fn text_center(
        &mut self,
        center_x: f64,
        y: f64,
        size: f64,
        font: Font,
        rgb: [u8; 3],
        s: &str,
    ) {
        let w = width(font, size, s);
        self.text(center_x - w / 2.0, y, size, font, rgb, s);
    }

    /// Text ending at `right`, baseline `y`.
    pub fn text_right(&mut self, right: f64, y: f64, size: f64, font: Font, rgb: [u8; 3], s: &str) {
        let w = width(font, size, s);
        self.text(right - w, y, size, font, rgb, s);
    }

    /// `s` in one `Tj`, shrunk from `size` until it fits `max_w` (down to 6 pt). A string that still
    /// does not fit is clipped to the box: the extraction is exact either way, and on paper an
    /// overflowing string would run into the next column instead.
    #[allow(clippy::too_many_arguments)]
    pub fn text_fit(
        &mut self,
        x: f64,
        y: f64,
        size: f64,
        font: Font,
        rgb: [u8; 3],
        s: &str,
        max_w: f64,
    ) {
        let mut size = size;
        while size > 6.0 && width(font, size, s) > max_w {
            size -= 0.25;
        }
        if width(font, size, s) > max_w {
            self.op(b"q\n");
            let h = size * 1.6;
            let top = y - size;
            self.raw(format!(
                "{} {} {} {} re W n\n",
                pt(x - 1.0),
                pt(A4_H - (top + h)),
                pt(max_w + 2.0),
                pt(h)
            ));
            self.text(x, y, size, font, rgb, s);
            self.op(b"Q\n");
        } else {
            self.text(x, y, size, font, rgb, s);
        }
    }

    fn raw(&mut self, s: String) {
        self.op(s.as_bytes());
    }

    /// Escape and write one PDF literal string (the bytes are WinAnsi).
    fn write_literal(&mut self, bytes: &[u8]) {
        for &b in bytes {
            match b {
                b'(' => self.op(b"\\("),
                b')' => self.op(b"\\)"),
                b'\\' => self.op(b"\\\\"),
                _ => self.op(&[b]),
            }
        }
    }

    /// A straight line.
    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, w: f64, rgb: [u8; 3]) {
        self.color_prefix(rgb, true);
        self.raw(format!(
            "{} {} m {} {} l {} w S\n",
            pt(x1),
            pt(A4_H - y1),
            pt(x2),
            pt(A4_H - y2),
            pt(w)
        ));
    }

    /// A rectangle; fill and/or stroke, skip neither.
    pub fn rect(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        fill: Option<[u8; 3]>,
        stroke: Option<(f64, [u8; 3])>,
    ) {
        if let Some(c) = fill {
            self.color_prefix(c, false);
        }
        if let Some((lw, c)) = stroke {
            self.color_prefix(c, true);
            self.raw(format!("{} w\n", pt(lw)));
        }
        self.raw(format!(
            "{} {} {} {} re\n",
            pt(x),
            pt(A4_H - (y + h)),
            pt(w),
            pt(h)
        ));
        match (fill, stroke) {
            (Some(_), Some(_)) => self.op(b"B\n"),
            (Some(_), None) => self.op(b"f\n"),
            _ => self.op(b"S\n"),
        }
    }

    /// A polyline through `pts` (at least two points), `w` pt wide.
    pub fn polyline(&mut self, pts: &[(f64, f64)], w: f64, rgb: [u8; 3]) {
        if pts.len() < 2 {
            return;
        }
        self.color_prefix(rgb, true);
        let mut first = true;
        let mut path = String::new();
        for (x, y) in pts {
            path.push_str(&format!(
                "{} {} {} ",
                pt(*x),
                pt(A4_H - *y),
                if first { "m" } else { "l" }
            ));
            first = false;
        }
        path.push_str(&format!("{} w S\n", pt(w)));
        self.raw(path);
    }

    /// A stroked circle of radius `r` around `(cx, cy)`.
    pub fn circle(&mut self, cx: f64, cy: f64, r: f64, w: f64, rgb: [u8; 3]) {
        self.circle_path(cx, cy, r);
        self.color_prefix(rgb, true);
        self.raw(format!("{} w S\n", pt(w)));
    }

    /// A filled circle.
    pub fn disc(&mut self, cx: f64, cy: f64, r: f64, rgb: [u8; 3]) {
        self.circle_path(cx, cy, r);
        self.color_prefix(rgb, false);
        self.op(b"f\n");
    }

    /// A filled square centred on `(cx, cy)`.
    pub fn square(&mut self, cx: f64, cy: f64, half: f64, rgb: [u8; 3]) {
        self.rect(
            cx - half,
            cy - half,
            half * 2.0,
            half * 2.0,
            Some(rgb),
            None,
        );
    }

    fn circle_path(&mut self, cx: f64, cy: f64, r: f64) {
        let y = A4_H - cy;
        let k = 0.552_284_75 * r;
        self.raw(format!(
            "{} {} m\n\
             {} {} {} {} {} {} c\n\
             {} {} {} {} {} {} c\n\
             {} {} {} {} {} {} c\n\
             {} {} {} {} {} {} c\n",
            pt(cx + r),
            pt(y),
            pt(cx + r),
            pt(y + k),
            pt(cx + k),
            pt(y + r),
            pt(cx),
            pt(y + r),
            pt(cx - k),
            pt(y + r),
            pt(cx - r),
            pt(y + k),
            pt(cx - r),
            pt(y),
            pt(cx - r),
            pt(y - k),
            pt(cx - k),
            pt(y - r),
            pt(cx),
            pt(y - r),
            pt(cx + k),
            pt(y - r),
            pt(cx + r),
            pt(y - k),
            pt(cx + r),
            pt(y),
        ));
    }

    /// Serialise the document: header, objects, xref, trailer.
    pub fn finish(self) -> Vec<u8> {
        let n = self.pages.len();
        // Object numbers: 1 catalog, 2 pages, 3..=2+n the pages, 3+n..=2+2n the content streams,
        // 3+2n..=5+2n the three fonts, 6+2n the info dictionary.
        let page_obj = |i: usize| 3 + i;
        let content_obj = |i: usize| 3 + n + i;
        // The three font objects are 3+2n, 4+2n, 5+2n (i is 1-based here).
        let font_obj = |i: usize| 2 + 2 * n + i;
        let info_obj = 6 + 2 * n;

        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
        let mut offsets: Vec<usize> = Vec::new();
        let push = |out: &mut Vec<u8>, body: Vec<u8>, offsets: &mut Vec<usize>| {
            offsets.push(out.len());
            out.extend_from_slice(&body);
        };

        let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", page_obj(i))).collect();
        push(
            &mut out,
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_vec(),
            &mut offsets,
        );
        push(
            &mut out,
            format!(
                "2 0 obj\n<< /Type /Pages /Kids [{}] /Count {} >>\nendobj\n",
                kids.join(" "),
                n
            )
            .into_bytes(),
            &mut offsets,
        );
        for i in 0..n {
            push(
                &mut out,
                format!(
                    "{} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] \
                     /Resources << /Font << /F1 {} 0 R /F2 {} 0 R /F3 {} 0 R >> >> /Contents {} 0 R >>\nendobj\n",
                    page_obj(i),
                    pt(A4_W),
                    pt(A4_H),
                    font_obj(1),
                    font_obj(2),
                    font_obj(3),
                    content_obj(i)
                )
                .into_bytes(),
                &mut offsets,
            );
        }
        for (i, page) in self.pages.into_iter().enumerate() {
            let mut body = format!(
                "{} 0 obj\n<< /Length {} >>\nstream\n",
                content_obj(i),
                page.ops.len()
            )
            .into_bytes();
            body.extend_from_slice(&page.ops);
            body.extend_from_slice(b"\nendstream\nendobj\n");
            push(&mut out, body, &mut offsets);
        }
        for (i, base) in [(1, "Helvetica"), (2, "Helvetica-Bold"), (3, "Courier")] {
            push(
                &mut out,
                format!(
                    "{} 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>\nendobj\n",
                    font_obj(i),
                    base
                )
                .into_bytes(),
                &mut offsets,
            );
        }
        // Escape the title for the info dictionary (it is a literal string too).
        let mut title = Vec::new();
        for b in encode(&self.title) {
            match b {
                b'(' => title.extend_from_slice(b"\\("),
                b')' => title.extend_from_slice(b"\\)"),
                b'\\' => title.extend_from_slice(b"\\\\"),
                _ => title.push(b),
            }
        }
        push(
            &mut out,
            {
                let mut body = format!("{} 0 obj\n<< /Title (", info_obj).into_bytes();
                body.extend_from_slice(&title);
                body.extend_from_slice(b") /Producer (Drafthouse cockpit) >>\nendobj\n");
                body
            },
            &mut offsets,
        );

        let xref_at = out.len();
        let count = info_obj + 1;
        out.extend_from_slice(format!("xref\n0 {count}\n").as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for offset in &offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {count} /Root 1 0 R /Info {info_obj} 0 R >>\nstartxref\n{xref_at}\n%%EOF\n"
            )
            .as_bytes(),
        );
        out
    }
}

/// A point value: two decimals, no exponent (a PDF number).
fn pt(v: f64) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A colour component, 0..1, three decimals.
fn rgb01(rgb: [u8; 3]) -> (String, String, String) {
    let one = |v: u8| {
        let s = format!("{:.3}", f64::from(v) / 255.0);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    (one(rgb[0]), one(rgb[1]), one(rgb[2]))
}

/// WinAnsi code for a char, or `None` when WinAnsi has no code for it. `≥`/`≤` are handled by
/// [`encode`], not here (they expand to two ASCII bytes).
fn winansi(c: char) -> Option<u8> {
    match c as u32 {
        0x20..=0x7e => Some(c as u8),
        0xa0 => Some(0xa0), // nbsp
        _ => match c {
            '¡' => Some(0xa1),
            '¢' => Some(0xa2),
            '£' => Some(0xa3),
            '¤' => Some(0xa4),
            '¥' => Some(0xa5),
            '¦' => Some(0xa6),
            '§' => Some(0xa7),
            '¨' => Some(0xa8),
            '©' => Some(0xa9),
            'ª' => Some(0xaa),
            '«' => Some(0xab),
            '¬' => Some(0xac),
            '\u{00ad}' => Some(0xad),
            '®' => Some(0xae),
            '¯' => Some(0xaf),
            '°' => Some(0xb0),
            '±' => Some(0xb1),
            '²' => Some(0xb2),
            '³' => Some(0xb3),
            '´' => Some(0xb4),
            'µ' => Some(0xb5),
            '¶' => Some(0xb6),
            '·' => Some(0xb7),
            '¸' => Some(0xb8),
            '¹' => Some(0xb9),
            'º' => Some(0xba),
            '»' => Some(0xbb),
            '¼' => Some(0xbc),
            '½' => Some(0xbd),
            '¾' => Some(0xbe),
            '¿' => Some(0xbf),
            'À'..='ÿ' => Some(c as u8),
            '€' => Some(0x80),
            '‚' => Some(0x82),
            'ƒ' => Some(0x83),
            '„' => Some(0x84),
            '…' => Some(0x85),
            '†' => Some(0x86),
            '‡' => Some(0x87),
            'ˆ' => Some(0x88),
            '‰' => Some(0x89),
            'Š' => Some(0x8a),
            '‹' => Some(0x8b),
            'Œ' => Some(0x8c),
            'Ž' => Some(0x8e),
            '‘' => Some(0x91),
            '’' => Some(0x92),
            '“' => Some(0x93),
            '”' => Some(0x94),
            '•' => Some(0x95),
            '–' => Some(0x96),
            '—' => Some(0x97),
            '˜' => Some(0x98),
            '™' => Some(0x99),
            'š' => Some(0x9a),
            '›' => Some(0x9b),
            'œ' => Some(0x9c),
            'ž' => Some(0x9e),
            'Ÿ' => Some(0x9f),
            _ => None,
        },
    }
}

/// Encode a string as WinAnsi bytes for a content stream. Documented substitutions: `≥` → `>=`,
/// `≤` → `<=`, `−` → `-`, any other unmapped char → `?`. Control characters (which would break a literal
/// string) become spaces.
pub fn encode(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '≥' => out.extend_from_slice(b">="),
            '≤' => out.extend_from_slice(b"<="),
            // issue #137: the house minus sign (U+2212) has no WinAnsi code; the hyphen-minus is its
            // printed form (× and ÷ are Latin-1, so they keep their own WinAnsi bytes)
            '\u{2212}' => out.push(b'-'),
            c if (c as u32) < 0x20 || c == '\u{7f}' => out.push(b' '),
            c => out.push(winansi(c).unwrap_or(b'?')),
        }
    }
    out
}

/// The char for one encoded byte, the reverse of [`encode`] - what [`extract_text`] decodes with.
pub fn decode(b: u8) -> char {
    match b {
        0x20..=0x7e => b as char,
        0xb0 => '°',
        0xb1 => '±',
        0xb2 => '²',
        0xb3 => '³',
        0xb5 => 'µ',
        0xb7 => '·',
        0xb9 => '¹',
        0xba => 'º',
        0xd7 => '×',
        0x96 => '–',
        0x97 => '—',
        0x85 => '…',
        0x91 => '‘',
        0x92 => '’',
        0x93 => '“',
        0x94 => '”',
        0x95 => '•',
        0xa0 => ' ',
        // Other Latin-1 letters decode to themselves (the sheet only uses the subset above).
        0x80..=0x9f | 0xa1..=0xff => char::from_u32(u32::from(b)).unwrap_or('?'),
        _ => '?',
    }
}

/* ------------------------------------------------------------------ widths */

/// Helvetica / Helvetica-Bold advance widths per 1000 em (Adobe AFM), ASCII plus the non-ASCII
/// subset [`encode`] can produce. Courier is a fixed 600. Anything not in the tables is 556 - an
/// estimate only used for alignment, never for a number's own text.
fn width_1000(font: Font, ch: char) -> f64 {
    if font == Font::Mono {
        return 600.0;
    }
    let bold = font == Font::Bold;
    match ch {
        ' ' => 278.0,
        '!' => {
            if bold {
                333.0
            } else {
                278.0
            }
        }
        '"' => {
            if bold {
                474.0
            } else {
                355.0
            }
        }
        '#' | '$' => 556.0,
        '%' => 889.0,
        '&' => {
            if bold {
                722.0
            } else {
                667.0
            }
        }
        '\'' => {
            if bold {
                238.0
            } else {
                191.0
            }
        }
        '(' | ')' => 333.0,
        '*' => 389.0,
        '+' => 584.0,
        ',' | '.' => 278.0,
        '-' => 333.0,
        '/' => 278.0,
        '0'..='9' => 556.0,
        ':' | ';' => {
            if bold {
                333.0
            } else {
                278.0
            }
        }
        '<' | '=' | '>' => 584.0,
        '?' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        '@' => {
            if bold {
                975.0
            } else {
                1015.0
            }
        }
        'A' => {
            if bold {
                722.0
            } else {
                667.0
            }
        }
        'B' => 722.0,
        'C' => 722.0,
        'D' => 722.0,
        'E' => 667.0,
        'F' => 611.0,
        'G' => 778.0,
        'H' => 722.0,
        'I' => 278.0,
        'J' => {
            if bold {
                556.0
            } else {
                500.0
            }
        }
        'K' => {
            if bold {
                722.0
            } else {
                667.0
            }
        }
        'L' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        'M' => 833.0,
        'N' => 722.0,
        'O' => 778.0,
        'P' => 667.0,
        'Q' => 778.0,
        'R' => 722.0,
        'S' => 667.0,
        'T' => 611.0,
        'U' => 722.0,
        'V' => 667.0,
        'W' => 944.0,
        'X' | 'Y' => 667.0,
        'Z' => 611.0,
        '[' | ']' => {
            if bold {
                333.0
            } else {
                278.0
            }
        }
        '\\' => 278.0,
        '^' => {
            if bold {
                584.0
            } else {
                469.0
            }
        }
        '_' => 556.0,
        '`' => 333.0,
        'a' => 556.0,
        'b' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        'c' => {
            if bold {
                556.0
            } else {
                500.0
            }
        }
        'd' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        'e' => 556.0,
        'f' => {
            if bold {
                333.0
            } else {
                278.0
            }
        }
        'g' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        'h' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        'i' => {
            if bold {
                278.0
            } else {
                222.0
            }
        }
        'j' => {
            if bold {
                278.0
            } else {
                222.0
            }
        }
        'k' => {
            if bold {
                556.0
            } else {
                500.0
            }
        }
        'l' => {
            if bold {
                278.0
            } else {
                222.0
            }
        }
        'm' => {
            if bold {
                889.0
            } else {
                833.0
            }
        }
        'n' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        'o' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        'p' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        'q' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        'r' => {
            if bold {
                389.0
            } else {
                333.0
            }
        }
        's' => {
            if bold {
                556.0
            } else {
                500.0
            }
        }
        't' => {
            if bold {
                333.0
            } else {
                278.0
            }
        }
        'u' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        'v' => {
            if bold {
                556.0
            } else {
                500.0
            }
        }
        'w' => {
            if bold {
                778.0
            } else {
                722.0
            }
        }
        'x' => {
            if bold {
                556.0
            } else {
                500.0
            }
        }
        'y' => {
            if bold {
                556.0
            } else {
                500.0
            }
        }
        'z' => 500.0,
        '{' | '}' => {
            if bold {
                389.0
            } else {
                334.0
            }
        }
        '|' => {
            if bold {
                280.0
            } else {
                260.0
            }
        }
        '~' => 584.0,
        '°' => 400.0,
        '±' => 584.0,
        '²' => 333.0,
        '³' => 333.0,
        '¹' => 333.0,
        'µ' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        '·' => 278.0,
        'º' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        '×' => 584.0,
        '–' => 556.0,
        '—' => 1000.0,
        '…' => 1000.0,
        '‘' | '’' => {
            if bold {
                278.0
            } else {
                222.0
            }
        }
        '“' | '”' => 333.0,
        '•' => 350.0,
        _ => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
    }
}

/// The advance width of `s` at `size` points, in points.
pub fn width(font: Font, size: f64, s: &str) -> f64 {
    s.chars()
        .map(|c| match c {
            '≥' | '≤' => width_1000(font, '>') + width_1000(font, '='),
            '\u{2212}' => width_1000(font, '-'),
            c => width_1000(font, c),
        })
        .sum::<f64>()
        * size
        / 1000.0
}

/// Wrap `s` at spaces so every line fits `max_w`; a single word longer than `max_w` stays on its own
/// line. Deterministic: the same string always wraps the same way.
pub fn wrap(font: Font, size: f64, s: &str, max_w: f64) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in s.split(' ') {
        let candidate = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if width(font, size, &candidate) <= max_w || line.is_empty() {
            line = candidate;
        } else {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/* ------------------------------------------------------------------ extraction */

/// Decode the text this module writes, out of a finished PDF: every content stream, every `Tj`
/// literal string, in order, one per line. It understands exactly the operators [`Pdf`] emits -
/// which is what makes it exact: no reader, no font program, no external tool, and it cannot
/// disagree with the writer about what a number is, because both are this file.
pub fn extract_text(pdf: &[u8]) -> Vec<String> {
    let mut lines = Vec::new();
    for stream in content_streams(pdf) {
        let mut at = 0usize;
        while at < stream.len() {
            match stream[at] {
                b'(' => {
                    let (text, next) = literal(&stream, at);
                    let mut probe = next;
                    while probe < stream.len() && stream[probe].is_ascii_whitespace() {
                        probe += 1;
                    }
                    if stream[probe..].starts_with(b"Tj") {
                        lines.push(text);
                    }
                    at = next;
                }
                _ => at += 1,
            }
        }
    }
    lines
}

/// The bytes of every `stream … endstream` segment.
fn content_streams(pdf: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(start) = find(&pdf[at..], b"stream\n") {
        let from = at + start + b"stream\n".len();
        let Some(end) = find(&pdf[from..], b"\nendstream") else {
            break;
        };
        out.push(pdf[from..from + end].to_vec());
        at = from + end;
    }
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// One literal string: its decoded contents and the index one past the closing paren.
fn literal(stream: &[u8], open: usize) -> (String, usize) {
    let mut text = String::new();
    let mut i = open + 1;
    while i < stream.len() {
        match stream[i] {
            b'\\' => {
                if i + 1 < stream.len() {
                    text.push(decode(stream[i + 1]));
                    i += 2;
                } else {
                    i += 1;
                }
            }
            b')' => return (text, i + 1),
            b => {
                text.push(decode(b));
                i += 1;
            }
        }
    }
    (text, i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_round_trips_through_the_extractor() {
        let mut pdf = Pdf::new("test");
        pdf.text(
            40.0,
            50.0,
            10.0,
            Font::Bold,
            [0, 0, 0],
            "cold water °C · 31.65",
        );
        pdf.line(40.0, 60.0, 200.0, 60.0, 0.5, [0, 0, 0]);
        pdf.rect(40.0, 70.0, 100.0, 20.0, Some([10, 20, 30]), None);
        pdf.circle(100.0, 120.0, 5.0, 1.0, [0, 0, 0]);
        pdf.text(
            40.0,
            150.0,
            9.0,
            Font::Mono,
            [0, 0, 0],
            "one (tricky) \\ string",
        );
        pdf.page();
        pdf.text(40.0, 50.0, 9.0, Font::Sans, [0, 0, 0], "page two");
        let bytes = pdf.finish();
        assert!(bytes.starts_with(b"%PDF-1.7"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        assert_eq!(
            extract_text(&bytes),
            vec![
                "cold water °C · 31.65",
                "one (tricky) \\ string",
                "page two",
            ]
        );
    }

    #[test]
    fn the_documented_substitutions_are_exact() {
        assert_eq!(encode("≥ 1.10"), b">= 1.10".to_vec());
        assert_eq!(encode("≤ 2"), b"<= 2".to_vec());
        // WinAnsi codes, not UTF-8: ³ = B3, · = B7, ° = B0, × = D7 (each one byte).
        assert_eq!(
            encode("m³/s · °C ×"),
            vec![0x6d, 0xb3, 0x2f, 0x73, 0x20, 0xb7, 0x20, 0xb0, 0x43, 0x20, 0xd7]
        );
        // and the extractor decodes them back
        assert_eq!(
            extract_text(&{
                let mut pdf = Pdf::new("t");
                pdf.text(0.0, 0.0, 9.0, Font::Sans, [0, 0, 0], "m³/s · °C ×");
                pdf.finish()
            }),
            vec!["m³/s · °C ×"]
        );
    }

    #[test]
    fn the_same_document_is_the_same_bytes() {
        let build = || {
            let mut pdf = Pdf::new("x");
            pdf.text(1.0, 2.0, 9.0, Font::Sans, [1, 2, 3], "hello");
            pdf.page();
            pdf.polyline(&[(0.0, 0.0), (10.0, 10.0)], 1.0, [0, 0, 0]);
            pdf.finish()
        };
        assert_eq!(build(), build());
    }

    #[test]
    fn xref_offsets_point_at_their_objects() {
        let mut pdf = Pdf::new("t");
        pdf.text(0.0, 0.0, 9.0, Font::Sans, [0, 0, 0], "a");
        let bytes = pdf.finish();
        let text = String::from_utf8_lossy(&bytes);
        let xref_at: usize = text
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(bytes[xref_at..].starts_with(b"xref"));
        // Every entry maps to its object: the free entry first, then objects 1..count-1 in order.
        // The `startxref` value is an offset into the original bytes, so the section is decoded
        // from there (it is plain ASCII, so the lossy decode is exact).
        let table = String::from_utf8_lossy(&bytes[xref_at..]);
        let mut lines = table.lines();
        assert_eq!(lines.next(), Some("xref"));
        let header = lines.next().expect("the subsection header");
        let (first, count) = header.split_once(' ').expect("`0 <count>`");
        assert_eq!(first, "0");
        let count: usize = count.parse().expect("the entry count");
        assert_eq!(lines.next(), Some("0000000000 65535 f "));
        for object in 1..count {
            let line = lines.next().expect("one entry per object");
            let offset: usize = line
                .split(' ')
                .next()
                .expect("an offset")
                .parse()
                .expect("a numeric offset");
            assert!(
                bytes[offset..].starts_with(format!("{object} 0 obj").as_bytes()),
                "the xref entry for object {object} points at {offset}"
            );
        }
        assert!(lines.next().expect("the trailer").starts_with("trailer"));
    }

    #[test]
    fn wrapping_keeps_every_line_inside_the_width() {
        let s = "The engine refused nothing: every record range this run passed through held.";
        for line in wrap(Font::Sans, 9.0, s, 200.0) {
            assert!(width(Font::Sans, 9.0, &line) <= 200.0, "{line}");
        }
    }
}
