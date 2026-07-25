use std::collections::HashMap;

/// Locale translation catalogue managing i18n dictionaries and fallback lookup.
#[derive(Clone, Debug)]
pub struct LocaleCatalogue {
    pub current_locale: String,
    pub fallback_locale: String,
    translations: HashMap<String, HashMap<String, String>>,
}

impl Default for LocaleCatalogue {
    fn default() -> Self {
        let mut catalogue = Self {
            current_locale: "en-US".to_string(),
            fallback_locale: "en-US".to_string(),
            translations: HashMap::new(),
        };

        let mut en_us = HashMap::new();
        en_us.insert("settings.title".to_string(), "Settings".to_string());
        en_us.insert("settings.general".to_string(), "General".to_string());
        en_us.insert("settings.appearance".to_string(), "Appearance".to_string());
        en_us.insert("settings.about".to_string(), "About".to_string());
        catalogue.translations.insert("en-US".to_string(), en_us);

        catalogue
    }
}

impl LocaleCatalogue {
    pub fn new(locale: impl Into<String>) -> Self {
        let mut catalogue = Self::default();
        catalogue.current_locale = locale.into();
        catalogue
    }

    pub fn insert_translation(&mut self, locale: &str, key: &str, value: &str) {
        self.translations
            .entry(locale.to_string())
            .or_default()
            .insert(key.to_string(), value.to_string());
    }

    pub fn tr(&self, key: &str) -> String {
        if let Some(dict) = self.translations.get(&self.current_locale)
            && let Some(val) = dict.get(key)
        {
            return val.clone();
        }

        if let Some(dict) = self.translations.get(&self.fallback_locale)
            && let Some(val) = dict.get(key)
        {
            return val.clone();
        }

        key.to_string()
    }

    pub fn pluralize(&self, count: usize, singular: &str, plural: &str) -> String {
        let pattern = if count == 1 { singular } else { plural };
        pattern.replace("{count}", &self.format_number(count))
    }

    pub fn format_number(&self, val: usize) -> String {
        let s = val.to_string();
        if self.current_locale.starts_with("bn") {
            s.chars()
                .map(|c| match c {
                    '0' => '০',
                    '1' => '১',
                    '2' => '২',
                    '3' => '৩',
                    '4' => '৪',
                    '5' => '৫',
                    '6' => '৬',
                    '7' => '৭',
                    '8' => '৮',
                    '9' => '৯',
                    other => other,
                })
                .collect()
        } else {
            s
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translation_catalogue_lookup_and_fallback() {
        let mut catalogue = LocaleCatalogue::new("bn-IN");
        catalogue.insert_translation("bn-IN", "settings.title", "সেটিংস");

        assert_eq!(catalogue.tr("settings.title"), "সেটিংস");
        assert_eq!(catalogue.tr("settings.general"), "General");
        assert_eq!(catalogue.tr("unknown.key"), "unknown.key");
    }

    #[test]
    fn test_pluralization_and_locale_number_formatting() {
        let en_cat = LocaleCatalogue::new("en-US");
        assert_eq!(
            en_cat.pluralize(1, "{count} window", "{count} windows"),
            "1 window"
        );
        assert_eq!(
            en_cat.pluralize(5, "{count} window", "{count} windows"),
            "5 windows"
        );

        let bn_cat = LocaleCatalogue::new("bn-IN");
        assert_eq!(bn_cat.format_number(12345), "১২৩৪৫");
    }
}
