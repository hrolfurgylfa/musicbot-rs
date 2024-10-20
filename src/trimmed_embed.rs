use std::{borrow::Cow, fmt::Write};

use serenity::all::{Colour, CreateEmbed, Embed, EmbedField, EmbedFooter, Timestamp};
use tracing::warn;

fn truncate_string_backwards(
    s: &mut String,
    max_len: usize,
    trim_str: &str,
    continue_: impl Fn(&str, usize) -> bool,
) {
    if max_len < trim_str.len() {
        warn!("Can't fit {:?} into the max_len.", trim_str);
        *s = "".to_owned();
        return;
    }
    if max_len >= s.len() {
        return;
    }
    let mut idx = max_len - trim_str.len();
    while continue_(&s, idx) {
        idx -= 1;
    }
    s.truncate(idx);
    *s += trim_str;
}

pub fn truncate_string_to_char_boundary(s: &mut String, max_len: usize) {
    truncate_string_backwards(s, max_len, "...", |s, idx| !s.is_char_boundary(idx));
}

pub fn truncate_string_to_newline_boundary(s: &mut String, max_len: usize) {
    truncate_string_backwards(s, max_len, "\n...", |s, idx| {
        s.as_bytes()[idx] != b'\n' && idx > 0
    });
}

const EMBED_MAX_SIZE: usize = 6000;

#[derive(Clone, Debug)]
pub struct Size(usize);

impl Size {
    pub fn new() -> Self {
        Self(0)
    }

    fn value(&self) -> usize {
        self.0
    }

    /// Tries to add a value and returns true if it fit within the EMBED_MAX_SIZE
    fn add(&mut self, val: usize, buffer: usize) -> bool {
        let new_val = self.0 + val;
        if new_val <= EMBED_MAX_SIZE - buffer {
            self.0 = new_val;
            return true;
        } else {
            return false;
        }
    }
}

pub struct TrimmedEmbed<'a> {
    too_big_msg: Cow<'static, str>,
    truncate_description_newline: bool,
    size: &'a mut Size,
}

#[allow(dead_code)]
impl<'a> TrimmedEmbed<'a> {
    fn make_builder(self) -> TrimmedEmbedBuilder<'a> {
        TrimmedEmbedBuilder::new(self)
    }

    pub fn new(size: &'a mut Size) -> Self {
        Self {
            too_big_msg: Cow::Borrowed("Too much data, some fields have been skipped."),
            truncate_description_newline: false,
            size,
        }
    }
    pub fn too_big_msg(mut self, s: impl Into<Cow<'static, str>>) -> Self {
        let too_big_msg = s.into();
        self.too_big_msg = too_big_msg;
        self
    }
    pub fn truncate_description_newline(mut self) -> Self {
        self.truncate_description_newline = true;
        self
    }

    pub fn title(self, s: impl Into<String>) -> TrimmedEmbedBuilder<'a> {
        self.make_builder().title(s)
    }
    pub fn description(self, s: impl Into<String>) -> TrimmedEmbedBuilder<'a> {
        self.make_builder().description(s)
    }
    pub fn fields(
        self,
        fields: impl IntoIterator<Item = (impl Into<String>, impl Into<String>, bool)>,
    ) -> TrimmedEmbedBuilder<'a> {
        self.make_builder().fields(fields)
    }
    pub fn field(
        self,
        name: impl Into<String>,
        value: impl Into<String>,
        inline: bool,
    ) -> TrimmedEmbedBuilder<'a> {
        self.make_builder().field(name, value, inline)
    }

    pub fn timestamp(self, timestamp: Timestamp) -> TrimmedEmbedBuilder<'a> {
        self.make_builder().timestamp(timestamp)
    }
    pub fn colour(self, colour: Colour) -> TrimmedEmbedBuilder<'a> {
        self.make_builder().colour(colour)
    }
    pub fn color(self, color: Colour) -> TrimmedEmbedBuilder<'a> {
        self.make_builder().color(color)
    }
}

pub struct TrimmedEmbedBuilder<'a> {
    embed: Embed,
    overflowed: bool,
    builder: TrimmedEmbed<'a>,
}

impl<'a> TrimmedEmbedBuilder<'a> {
    fn too_big_msg_length(&self) -> usize {
        let msg_len = self.builder.too_big_msg.len();
        if msg_len == 0 {
            return 0;
        } else {
            return msg_len + 1;
        }
    }

    fn new(builder: TrimmedEmbed<'a>) -> Self {
        Self {
            embed: Embed::default(),
            builder,
            overflowed: false,
        }
    }
    pub fn title(mut self, s: impl Into<String>) -> Self {
        let mut s = s.into();
        truncate_string_to_char_boundary(&mut s, 256);

        if self.builder.size.add(s.len(), self.too_big_msg_length()) {
            self.embed.title = Some(s);
        } else {
            self.overflowed = true;
        }
        self
    }
    pub fn description(mut self, s: impl Into<String>) -> Self {
        let mut s = s.into();
        if self.builder.truncate_description_newline {
            truncate_string_to_newline_boundary(&mut s, 4096.max(self.builder.size.value()));
        } else {
            truncate_string_to_char_boundary(&mut s, 4096.max(self.builder.size.value()));
        }
        if self.builder.size.add(s.len(), self.too_big_msg_length()) {
            self.embed.description = Some(s);
        } else {
            self.overflowed = true;
        }
        self
    }
    pub fn fields(
        mut self,
        fields: impl IntoIterator<Item = (impl Into<String>, impl Into<String>, bool)>,
    ) -> Self {
        for (name, value, inline) in fields.into_iter() {
            let (mut name, mut value): (String, String) = (name.into(), value.into());
            truncate_string_to_char_boundary(&mut name, 256);
            truncate_string_to_char_boundary(&mut value, 1024);
            if self
                .builder
                .size
                .add(name.len() + value.len(), self.too_big_msg_length())
            {
                self.embed.fields.push(EmbedField::new(name, value, inline));
            } else {
                self.overflowed = true;
                break;
            }
        }
        self
    }
    pub fn field(self, name: impl Into<String>, value: impl Into<String>, inline: bool) -> Self {
        self.fields([(name, value, inline)])
    }

    pub fn timestamp(mut self, timestamp: Timestamp) -> Self {
        self.embed.timestamp = Some(timestamp);
        self
    }
    pub fn colour(mut self, colour: Colour) -> Self {
        self.embed.colour = Some(colour);
        self
    }
    pub fn color(self, color: Colour) -> Self {
        self.colour(color)
    }
}

/// This is so cursed, why can't I just call EmbedFooter::new()?
fn create_embed_footer(text: &str) -> EmbedFooter {
    let toml = toml::toml! {name = text};
    let toml_str = toml::to_string(&toml).unwrap();
    toml::from_str::<EmbedFooter>(&toml_str).unwrap()
}

impl Into<Embed> for TrimmedEmbed<'_> {
    fn into(self) -> Embed {
        self.make_builder().into()
    }
}

impl Into<CreateEmbed> for TrimmedEmbed<'_> {
    fn into(self) -> CreateEmbed {
        self.make_builder().into()
    }
}

impl Into<Embed> for TrimmedEmbedBuilder<'_> {
    fn into(mut self) -> Embed {
        if !self.overflowed {
            return self.embed;
        };
        if let Some(footer) = &mut self.embed.footer {
            write!(footer.text, "\n{}", self.builder.too_big_msg.as_ref()).unwrap();
        } else {
            let footer = create_embed_footer(self.builder.too_big_msg.as_ref());
            self.embed.footer = Some(footer);
        }

        self.embed
    }
}

impl Into<CreateEmbed> for TrimmedEmbedBuilder<'_> {
    fn into(self) -> CreateEmbed {
        let embed: Embed = self.into();
        embed.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! truncate_string_to_boundary_tests {
        ($function:ident,$($name:ident: $value:expr,)*) => {
            mod $function {
            use super::*;
            $(
                #[test]
                fn $name() {
                    let (input_str, truncate_to, expected_str) = $value;
                    let mut input = input_str.to_owned();
                    $function(&mut input, truncate_to);
                    assert_eq!(input, expected_str.to_owned());
                }
            )*
            }
        }
    }

    truncate_string_to_boundary_tests!(
        truncate_string_to_char_boundary,
        trim_to_zero: ("🪾abcde", 3, "..."),
        start_inside_char_1: ("🪾abcde", 4, "..."),
        start_inside_char_2: ("🪾abcde", 5, "..."),
        start_inside_char_3: ("🪾abcde", 6, "..."),
        start_on_multi_char_edge: ("🪾abcde", 7, "🪾..."),
        start_on_string_ascii_char: ("🪾abcde", 8, "🪾a..."),
        start_past_string_end: ("🪾abcde", 9, "🪾abcde"),
    );

    truncate_string_to_boundary_tests!(
        truncate_string_to_newline_boundary,
        no_newline: ("abcde", 4, "\n..."),
        start_at_newline: ("abcd\nabcdef", 8, "abcd\n..."),
        start_1_before_newline: ("abcd\nabcdef", 9, "abcd\n..."),
        full_string_fits: ("abcd\nabcdef", 11, "abcd\nabcdef"),
        more_than_string_fits: ("abcd\nabcdef", 15, "abcd\nabcdef"),
    );
}
