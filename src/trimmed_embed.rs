use core::panic;
use std::{borrow::Cow, fmt::Write};

use serenity::all::{Colour, CreateEmbed, Embed, EmbedField, EmbedFooter, Timestamp};

fn truncate_string_backwards(
    s: &mut String,
    max_len: usize,
    trim_str: &str,
    continue_: impl Fn(&str, usize) -> bool,
) {
    if max_len < trim_str.len() {
        panic!("Can't fit {:?} into the max_len.", trim_str);
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

pub struct TrimmedEmbed {
    too_big_msg: Cow<'static, str>,
    truncate_description_newline: bool,
}

#[allow(dead_code)]
impl TrimmedEmbed {
    fn make_builder(self) -> TrimmedEmbedBuilder {
        TrimmedEmbedBuilder::new(self)
    }

    pub fn new() -> Self {
        Self {
            too_big_msg: Cow::Borrowed("Too much data, some fields have been skipped."),
            truncate_description_newline: false,
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

    pub fn title(self, s: impl Into<String>) -> TrimmedEmbedBuilder {
        self.make_builder().title(s)
    }
    pub fn description(self, s: impl Into<String>) -> TrimmedEmbedBuilder {
        self.make_builder().description(s)
    }
    pub fn fields(
        self,
        fields: impl IntoIterator<Item = (impl Into<String>, impl Into<String>, bool)>,
    ) -> TrimmedEmbedBuilder {
        self.make_builder().fields(fields)
    }
    pub fn field(
        self,
        name: impl Into<String>,
        value: impl Into<String>,
        inline: bool,
    ) -> TrimmedEmbedBuilder {
        self.make_builder().field(name, value, inline)
    }

    pub fn timestamp(self, timestamp: Timestamp) -> TrimmedEmbedBuilder {
        self.make_builder().timestamp(timestamp)
    }
    pub fn colour(self, colour: Colour) -> TrimmedEmbedBuilder {
        self.make_builder().colour(colour)
    }
    pub fn color(self, color: Colour) -> TrimmedEmbedBuilder {
        self.make_builder().color(color)
    }
}

pub struct TrimmedEmbedBuilder {
    embed: Embed,
    size: usize,
    overflowed: bool,
    builder: TrimmedEmbed,
}

impl TrimmedEmbedBuilder {
    fn max_length(&self) -> usize {
        6000 - self.too_big_msg_length()
    }
    fn too_big_msg_length(&self) -> usize {
        let msg_len = self.builder.too_big_msg.len();
        if msg_len == 0 {
            return 0;
        } else {
            return msg_len + 1;
        }
    }

    fn new(builder: TrimmedEmbed) -> Self {
        Self {
            embed: Embed::default(),
            size: 0,
            builder,
            overflowed: false,
        }
    }
    pub fn title(mut self, s: impl Into<String>) -> Self {
        let mut s = s.into();
        truncate_string_to_char_boundary(&mut s, 256);
        let new_size = self.size + s.len();
        if new_size <= self.max_length() {
            self.size = new_size;
            self.embed.title = Some(s);
        } else {
            self.overflowed = true;
        }
        self
    }
    pub fn description(mut self, s: impl Into<String>) -> Self {
        let mut s = s.into();
        if self.builder.truncate_description_newline {
            truncate_string_to_newline_boundary(&mut s, 2048);
        } else {
            truncate_string_to_char_boundary(&mut s, 2048);
        }
        let new_size = self.size + s.len();
        if new_size <= self.max_length() {
            self.size = new_size;
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
            let new_size = self.size + name.len() + value.len();
            if new_size <= self.max_length() {
                self.size = new_size;
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

impl Into<Embed> for TrimmedEmbed {
    fn into(self) -> Embed {
        self.make_builder().into()
    }
}

impl Into<CreateEmbed> for TrimmedEmbed {
    fn into(self) -> CreateEmbed {
        self.make_builder().into()
    }
}

impl Into<Embed> for TrimmedEmbedBuilder {
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

impl Into<CreateEmbed> for TrimmedEmbedBuilder {
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
