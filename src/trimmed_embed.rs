use serenity::all::{Colour, CreateEmbed, Embed, EmbedField, EmbedFooter, Timestamp};

pub fn truncate_string_to_char_boundary(s: &mut String, max_len: usize) {
    if max_len >= s.len() {
        return;
    }
    let mut idx = max_len;
    while !s.is_char_boundary(idx) {
        idx -= 1;
    }
    s.truncate(idx);
}

pub struct TrimmedEmbed {
    embed: Embed,
    size: usize,
    too_big_msg: Option<String>,
    overflowed: bool,
}

impl TrimmedEmbed {
    fn max_length(&self) -> usize {
        6000 - self.too_big_msg.as_ref().map(|msg| msg.len()).unwrap_or(0)
    }

    pub fn new() -> TrimmedEmbed {
        TrimmedEmbed {
            embed: Embed::default(),
            size: 0,
            too_big_msg: Some("\nToo much data, some fields have been skipped.".to_owned()),
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
        truncate_string_to_char_boundary(&mut s, 2048);
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
    fn into(mut self) -> Embed {
        if !self.overflowed {
            return self.embed;
        };
        let Some(too_big_msg) = self.too_big_msg else {
            return self.embed;
        };
        if let Some(footer) = &mut self.embed.footer {
            footer.text += &too_big_msg;
        } else {
            let footer = create_embed_footer(&too_big_msg);
            self.embed.footer = Some(footer);
        }

        self.embed
    }
}

impl Into<CreateEmbed> for TrimmedEmbed {
    fn into(self) -> CreateEmbed {
        let embed: Embed = self.into();
        embed.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_string_to_char_boundary() {
        let mut a = "🪾f".to_owned();
        truncate_string_to_char_boundary(&mut a, 0);
        assert_eq!(a, "".to_owned());

        let mut a = "🪾f".to_owned();
        truncate_string_to_char_boundary(&mut a, 1);
        assert_eq!(a, "".to_owned());

        let mut a = "🪾f".to_owned();
        truncate_string_to_char_boundary(&mut a, 2);
        assert_eq!(a, "".to_owned());

        let mut a = "🪾f".to_owned();
        truncate_string_to_char_boundary(&mut a, 3);
        assert_eq!(a, "".to_owned());

        let mut a = "🪾f".to_owned();
        truncate_string_to_char_boundary(&mut a, 4);
        assert_eq!(a, "🪾".to_owned());

        let mut a = "🪾f".to_owned();
        truncate_string_to_char_boundary(&mut a, 5);
        assert_eq!(a, "🪾f".to_owned());

        let mut a = "🪾f".to_owned();
        truncate_string_to_char_boundary(&mut a, 6);
        assert_eq!(a, "🪾f".to_owned());
    }
}
