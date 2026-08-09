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
        en_us.insert(
            "overview.search_placeholder".to_string(),
            "Search, calculate or run".to_string(),
        );
        catalogue.translations.insert("en-US".to_string(), en_us);

        let mut bn_in = HashMap::new();
        bn_in.insert("settings.title".to_string(), "সেটিংস".to_string());
        bn_in.insert("settings.general".to_string(), "সাধারণ".to_string());
        bn_in.insert("settings.appearance".to_string(), "চেহারা".to_string());
        bn_in.insert("settings.about".to_string(), "সম্পর্কে".to_string());
        bn_in.insert(
            "overview.search_placeholder".to_string(),
            "খুঁজুন, গণনা করুন বা চালান...".to_string(),
        );
        catalogue.translations.insert("bn-IN".to_string(), bn_in);

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

    pub fn is_rtl(&self) -> bool {
        let lang = self
            .current_locale
            .split(&['-', '_'][..])
            .next()
            .unwrap_or("")
            .to_lowercase();
        matches!(lang.as_str(), "ar" | "he" | "fa" | "ur")
    }

    pub fn truncate_or_expand(&self, text: &str, max_len: usize) -> String {
        if text.chars().count() <= max_len {
            text.to_string()
        } else {
            let truncated: String = text.chars().take(max_len.saturating_sub(1)).collect();
            format!("{truncated}…")
        }
    }

    pub fn workspace_label(&self, id: u64, custom_name: Option<&str>) -> String {
        if let Some(name) = custom_name
            && !name.trim().is_empty()
        {
            return name.to_string();
        }
        if self.current_locale.starts_with("bn") {
            format!("ওয়ার্কস্পেস {}", self.format_number(id as usize))
        } else {
            format!("Workspace {id}")
        }
    }

    /// Logs assistive technology accessibility status announcements for screen readers & overlays.
    pub fn announce_status(&self, message: &str) -> String {
        tracing::info!(locale = %self.current_locale, message = %message, "Accessibility Status Announcement");
        format!("[Announce ({})] {message}", self.current_locale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translation_catalogue_lookup_and_fallback() {
        let mut catalogue = LocaleCatalogue::new("bn-IN");
        catalogue.insert_translation("bn-IN", "custom.title", "কাস্টম");
        catalogue.insert_translation("en-US", "only_en.key", "English Fallback");

        assert_eq!(catalogue.tr("custom.title"), "কাস্টম");
        assert_eq!(catalogue.tr("only_en.key"), "English Fallback");
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

    #[test]
    fn test_bidirectional_layout_and_string_expansion() {
        let ar_cat = LocaleCatalogue::new("ar-SA");
        assert!(ar_cat.is_rtl());

        let en_cat = LocaleCatalogue::new("en-US");
        assert!(!en_cat.is_rtl());

        let long_str = "Long string expansion test";
        assert_eq!(en_cat.truncate_or_expand(long_str, 10), "Long stri…");
        assert_eq!(en_cat.truncate_or_expand("Short", 10), "Short");
    }

    #[test]
    fn test_localized_workspace_fallback_labels() {
        let en_cat = LocaleCatalogue::new("en-US");
        assert_eq!(en_cat.workspace_label(1, None), "Workspace 1");
        assert_eq!(en_cat.workspace_label(1, Some("Media")), "Media");

        let bn_cat = LocaleCatalogue::new("bn-IN");
        assert_eq!(bn_cat.workspace_label(2, None), "ওয়ার্কস্পেস ২");
    }

    #[test]
    fn test_production_localization_catalogue_resources() {
        let catalogue = LocaleCatalogue::new("bn-IN");
        assert_eq!(catalogue.tr("settings.title"), "সেটিংস");
        assert_eq!(
            catalogue.tr("overview.search_placeholder"),
            "খুঁজুন, গণনা করুন বা চালান..."
        );
    }

    #[test]
    fn test_assistive_tech_overlay_announcements() {
        let catalogue = LocaleCatalogue::new("en-US");
        let announcement = catalogue.announce_status("Workspace overview opened");
        assert_eq!(announcement, "[Announce (en-US)] Workspace overview opened");
    }
}
