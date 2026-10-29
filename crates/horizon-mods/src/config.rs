//! Capability `config`'s settings (docs/mods.md, "Config"): the entries a
//! server mod declares, their values, and the TOML file they live in
//! (`<mods folder>/config/<id>.toml`), which a player can edit by hand while
//! the game runs. The file carries each entry's description and range as
//! comments; a value that is missing or wrong is its default again.
use std::collections::BTreeMap;

/// Entries one mod may declare.
pub const MAX_ENTRIES: usize = 128;
/// The folder, beside the mod's own file, the config files are kept in.
pub const DIR: &str = "config";

/// What an entry holds and the limits it keeps.
#[derive(Clone, Debug, PartialEq)]
pub enum Spec {
    Boolean(bool),
    Integer {
        default: i64,
        min: i64,
        max: i64,
    },
    Number {
        default: f64,
        min: f64,
        max: f64,
    },
    Text(String),
    Choice {
        default: String,
        options: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Boolean(bool),
    Integer(i64),
    Number(f64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub key: String,
    pub description: String,
    pub spec: Spec,
}

impl Spec {
    pub fn default_value(&self) -> Value {
        match self {
            Spec::Boolean(b) => Value::Boolean(*b),
            Spec::Integer { default, .. } => Value::Integer(*default),
            Spec::Number { default, .. } => Value::Number(*default),
            Spec::Text(t) => Value::Text(t.clone()),
            Spec::Choice { default, .. } => Value::Text(default.clone()),
        }
    }

    /// `Ok(value)` when `value` is of this kind and inside its limits; the
    /// reason when not.
    pub fn check(&self, value: &Value) -> Result<Value, String> {
        match (self, value) {
            (Spec::Boolean(_), Value::Boolean(_)) => Ok(value.clone()),
            (Spec::Integer { min, max, .. }, Value::Integer(n)) => {
                if (min..=max).contains(&n) {
                    Ok(value.clone())
                } else {
                    Err(format!("{n} is not {min} to {max}"))
                }
            }
            (Spec::Number { min, max, .. }, Value::Number(n)) => {
                if n.is_finite() && (*min..=*max).contains(n) {
                    Ok(value.clone())
                } else {
                    Err(format!("{n} is not {min} to {max}"))
                }
            }
            // A whole number where a fractional one is wanted.
            (Spec::Number { .. }, Value::Integer(n)) => self.check(&Value::Number(*n as f64)),
            (Spec::Text(_), Value::Text(t)) => {
                if t.len() <= MAX_TEXT {
                    Ok(value.clone())
                } else {
                    Err(format!("longer than {MAX_TEXT} bytes"))
                }
            }
            (Spec::Choice { options, .. }, Value::Text(t)) => {
                if options.contains(t) {
                    Ok(value.clone())
                } else {
                    Err(format!("{t:?} is not one of {}", options.join(", ")))
                }
            }
            _ => Err("is the wrong kind of value".into()),
        }
    }

    /// The note under an entry's description in the file.
    fn note(&self) -> String {
        match self {
            Spec::Boolean(d) => format!("true or false. Default: {d}"),
            Spec::Integer { default, min, max } => {
                format!("{min} to {max}. Default: {default}")
            }
            Spec::Number { default, min, max } => {
                format!("{min:?} to {max:?}. Default: {default:?}")
            }
            Spec::Text(d) => format!("Text. Default: {d:?}"),
            Spec::Choice { default, options } => {
                format!("One of {}. Default: {default}", options.join(", "))
            }
        }
    }
}

/// The longest text value, in bytes.
pub const MAX_TEXT: usize = 1024;
const MAX_DESCRIPTION: usize = 512;

/// Whether `key` is a config key: `[a-z0-9_.-]{1,64}`.
pub fn key_ok(key: &str) -> bool {
    (1..=64).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
}

/// An entry's rules, checked once at `declare`.
fn check_entry(e: &Entry) -> Result<(), String> {
    if !key_ok(&e.key) {
        return Err(format!(
            "{:?} is not a config key ([a-z0-9_.-]{{1,64}})",
            e.key
        ));
    }
    if e.description.len() > MAX_DESCRIPTION {
        return Err(format!(
            "{}: the description is over {MAX_DESCRIPTION} bytes",
            e.key
        ));
    }
    let bad = |why: &str| Err(format!("{}: {why}", e.key));
    match &e.spec {
        Spec::Boolean(_) => {}
        Spec::Integer { default, min, max } => {
            if min > max || !(min..=max).contains(&default) {
                return bad("the default is outside min..=max");
            }
        }
        Spec::Number { default, min, max } => {
            if ![default, min, max].iter().all(|v| v.is_finite())
                || min > max
                || !(min..=max).contains(&default)
            {
                return bad("the default is outside min..=max, or a limit is not finite");
            }
        }
        Spec::Text(d) => {
            if d.len() > MAX_TEXT {
                return bad("the default text is too long");
            }
        }
        Spec::Choice { default, options } => {
            if options.is_empty() || options.len() > 32 {
                return bad("a choice has 1 to 32 options");
            }
            if options.iter().any(|o| o.is_empty() || o.len() > 64) {
                return bad("an option is 1 to 64 bytes");
            }
            if !options.contains(default) {
                return bad("the default is not one of the options");
            }
        }
    }
    Ok(())
}

/// A mod's declared entries and their values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Config {
    entries: Vec<Entry>,
    values: BTreeMap<String, Value>,
    /// What the file held when it was last read or written: a file that is
    /// the same needs no reading.
    last_text: Option<String>,
}

fn from_toml(spec: &Spec, v: &toml::Value) -> Option<Value> {
    let value = match (spec, v) {
        (Spec::Boolean(_), toml::Value::Boolean(b)) => Value::Boolean(*b),
        (Spec::Integer { .. }, toml::Value::Integer(n)) => Value::Integer(*n),
        (Spec::Number { .. }, toml::Value::Float(n)) => Value::Number(*n),
        (Spec::Number { .. }, toml::Value::Integer(n)) => Value::Number(*n as f64),
        (Spec::Text(_) | Spec::Choice { .. }, toml::Value::String(s)) => Value::Text(s.clone()),
        _ => return None,
    };
    spec.check(&value).ok()
}

fn to_toml(v: &Value) -> toml::Value {
    match v {
        Value::Boolean(b) => toml::Value::Boolean(*b),
        Value::Integer(n) => toml::Value::Integer(*n),
        Value::Number(n) => toml::Value::Float(*n),
        Value::Text(t) => toml::Value::String(t.clone()),
    }
}

/// What reading the file did.
#[derive(Debug, Default, PartialEq)]
pub struct Reload {
    /// The keys whose value changed.
    pub changed: Vec<String>,
    /// The file as it should now be, when it is not what was read (a value
    /// was put right, or an entry is new): the caller writes it.
    pub rewrite: Option<String>,
    /// What was wrong in the file, for the log.
    pub problems: Vec<String>,
}

impl Config {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// Replaces the entries with `entries`, checked, and reads `text` (the
    /// file's content, `None` where there is none) against them.
    pub fn declare(&mut self, entries: Vec<Entry>, text: Option<&str>) -> Result<Reload, String> {
        if entries.len() > MAX_ENTRIES {
            return Err(format!("at most {MAX_ENTRIES} entries"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for e in &entries {
            check_entry(e)?;
            if !seen.insert(&e.key) {
                return Err(format!("{} is declared twice", e.key));
            }
        }
        let previous = std::mem::take(&mut self.values);
        self.entries = entries;
        self.values = self
            .entries
            .iter()
            .map(|e| (e.key.clone(), e.spec.default_value()))
            .collect();
        self.last_text = None;
        let mut reload = self.read(text);
        // What a restarted mod held is what it holds again, but only what
        // the file says counts: `previous` is for a second declare.
        reload
            .changed
            .retain(|k| previous.get(k) != self.values.get(k));
        Ok(reload)
    }

    /// Reads the file's `text` (`None`: there is no file) into the values:
    /// a value that is missing or wrong is the default again.
    pub fn read(&mut self, text: Option<&str>) -> Reload {
        let mut out = Reload::default();
        if text.is_some() && text == self.last_text.as_deref() {
            return out;
        }
        let table = match text {
            None => toml::Table::new(),
            Some(t) => match t.parse::<toml::Table>() {
                Ok(table) => table,
                Err(e) => {
                    // A typo is the player's to mend: keep what is held
                    // and leave the file alone.
                    out.problems.push(format!("the file does not parse: {e}"));
                    self.last_text = text.map(str::to_owned);
                    return out;
                }
            },
        };
        let mut put_right = text.is_none();
        for e in &self.entries {
            let found = table.get(&e.key);
            let value = match found.and_then(|v| from_toml(&e.spec, v)) {
                Some(v) => v,
                None => {
                    if let Some(v) = found {
                        out.problems.push(format!(
                            "{} = {v} is not valid ({}): using the default",
                            e.key,
                            e.spec.note()
                        ));
                    }
                    put_right = true;
                    e.spec.default_value()
                }
            };
            if self.values.get(&e.key) != Some(&value) {
                out.changed.push(e.key.clone());
                self.values.insert(e.key.clone(), value);
            }
        }
        let rendered = self.render();
        // A file with the right values but other comments, or an extra
        // key, is the player's; only a value put right is rewritten.
        if put_right || text.is_some_and(|t| missing_entries(&self.entries, &table, t)) {
            self.last_text = Some(rendered.clone());
            out.rewrite = Some(rendered);
        } else {
            self.last_text = text.map(str::to_owned);
        }
        out
    }

    /// Sets `key` to `value`, checked; the file's text after it.
    pub fn set(&mut self, key: &str, value: &Value) -> Result<String, String> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.key == key)
            .ok_or_else(|| format!("no config entry {key:?}"))?;
        let value = entry
            .spec
            .check(value)
            .map_err(|why| format!("{key}: {why}"))?;
        self.values.insert(key.to_owned(), value);
        let rendered = self.render();
        self.last_text = Some(rendered.clone());
        Ok(rendered)
    }

    /// The file for the entries and values held: each entry with its
    /// description and limits as comments.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (i, e) in self.entries.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            for line in e.description.lines() {
                out.push_str(&format!("# {line}\n"));
            }
            out.push_str(&format!("# {}\n", e.spec.note()));
            let v = self
                .values
                .get(&e.key)
                .cloned()
                .unwrap_or_else(|| e.spec.default_value());
            out.push_str(&format!("{} = {}\n", e.key, to_toml(&v)));
        }
        out
    }
}

/// Whether a declared key has no line in the file's text.
fn missing_entries(entries: &[Entry], table: &toml::Table, _text: &str) -> bool {
    entries.iter().any(|e| !table.contains_key(&e.key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<Entry> {
        vec![
            Entry {
                key: "charge".into(),
                description: "Energy a cell makes each tick.".into(),
                spec: Spec::Integer {
                    default: 5,
                    min: 1,
                    max: 100,
                },
            },
            Entry {
                key: "factor".into(),
                description: "A fraction.".into(),
                spec: Spec::Number {
                    default: 0.5,
                    min: 0.0,
                    max: 1.0,
                },
            },
            Entry {
                key: "loud".into(),
                description: "Whether it shouts.".into(),
                spec: Spec::Boolean(false),
            },
            Entry {
                key: "name".into(),
                description: "What it is called.".into(),
                spec: Spec::Text("Sentinel".into()),
            },
            Entry {
                key: "sound".into(),
                description: "Which chime.".into(),
                spec: Spec::Choice {
                    default: "chime".into(),
                    options: vec!["chime".into(), "bell".into()],
                },
            },
        ]
    }

    #[test]
    fn declaring_without_a_file_gives_the_defaults_and_a_file_to_write() {
        let mut c = Config::default();
        let r = c.declare(entries(), None).unwrap();
        assert_eq!(c.get("charge"), Some(&Value::Integer(5)));
        assert_eq!(c.get("sound"), Some(&Value::Text("chime".into())));
        assert!(r.changed.is_empty());
        let file = r.rewrite.expect("a file to write");
        assert!(
            file.contains("# Energy a cell makes each tick.\n# 1 to 100. Default: 5\ncharge = 5\n")
        );
        assert!(file.contains("factor = 0.5\n"));
        assert!(file.contains("name = \"Sentinel\"\n"));
        // What was written reads back as the same, and needs no rewrite.
        let mut again = Config::default();
        let r = again.declare(entries(), Some(&file)).unwrap();
        assert_eq!(r, Reload::default());
        assert_eq!(again, c);
    }

    #[test]
    fn a_hand_edit_changes_values_and_a_wrong_one_is_put_right() {
        let mut c = Config::default();
        let file = c.declare(entries(), None).unwrap().rewrite.unwrap();
        let edited = file
            .replace("charge = 5", "charge = 50")
            .replace("loud = false", "loud = true")
            .replace("factor = 0.5", "factor = 7.0")
            .replace("sound = \"chime\"", "sound = \"gong\"");
        let r = c.read(Some(&edited));
        assert_eq!(r.changed, vec!["charge".to_string(), "loud".to_string()]);
        assert_eq!(c.get("charge"), Some(&Value::Integer(50)));
        assert_eq!(c.get("loud"), Some(&Value::Boolean(true)));
        // Out of range and not an option: the default, said and rewritten.
        assert_eq!(c.get("factor"), Some(&Value::Number(0.5)));
        assert_eq!(c.get("sound"), Some(&Value::Text("chime".into())));
        assert_eq!(r.problems.len(), 2);
        let rewritten = r.rewrite.unwrap();
        assert!(rewritten.contains("charge = 50\n"));
        assert!(rewritten.contains("factor = 0.5\n"));
        // The same file again is nothing new.
        assert_eq!(c.read(Some(&rewritten)), Reload::default());
    }

    #[test]
    fn a_whole_number_is_a_number_and_the_wrong_kind_is_not() {
        let mut c = Config::default();
        c.declare(entries(), None).unwrap();
        let r = c.read(Some("factor = 1\ncharge = 2.5\nname = 4\n"));
        assert_eq!(c.get("factor"), Some(&Value::Number(1.0)));
        assert_eq!(c.get("charge"), Some(&Value::Integer(5)));
        assert_eq!(c.get("name"), Some(&Value::Text("Sentinel".into())));
        assert_eq!(r.problems.len(), 2);
    }

    #[test]
    fn a_file_that_does_not_parse_is_left_alone() {
        let mut c = Config::default();
        c.declare(entries(), None).unwrap();
        c.set("charge", &Value::Integer(9)).unwrap();
        let r = c.read(Some("charge = = 4\n"));
        assert_eq!(c.get("charge"), Some(&Value::Integer(9)));
        assert!(r.rewrite.is_none());
        assert_eq!(r.problems.len(), 1);
        // Told once: the same text again is quiet.
        assert!(c.read(Some("charge = = 4\n")).problems.is_empty());
    }

    #[test]
    fn a_new_entry_is_added_to_a_file_the_player_kept() {
        let mut c = Config::default();
        let r = c
            .declare(entries(), Some("# my notes\ncharge = 7\n"))
            .unwrap();
        assert_eq!(c.get("charge"), Some(&Value::Integer(7)));
        assert_eq!(r.changed, vec!["charge".to_string()]);
        let file = r.rewrite.unwrap();
        assert!(file.contains("charge = 7\n") && file.contains("loud = false\n"));
    }

    #[test]
    fn set_checks_and_renders() {
        let mut c = Config::default();
        c.declare(entries(), None).unwrap();
        assert!(c.set("charge", &Value::Integer(500)).is_err());
        assert!(c.set("charge", &Value::Number(5.0)).is_err());
        assert!(c.set("nothing", &Value::Integer(1)).is_err());
        assert!(c.set("sound", &Value::Text("gong".into())).is_err());
        let file = c.set("sound", &Value::Text("bell".into())).unwrap();
        assert!(file.contains("sound = \"bell\"\n"));
        assert_eq!(c.get("sound"), Some(&Value::Text("bell".into())));
        // A whole number for a fractional entry.
        c.set("factor", &Value::Integer(1)).unwrap();
        assert_eq!(c.get("factor"), Some(&Value::Number(1.0)));
    }

    #[test]
    fn declare_refuses_bad_entries() {
        let one = |e: Entry| Config::default().declare(vec![e], None);
        let int = |default, min, max| Entry {
            key: "n".into(),
            description: String::new(),
            spec: Spec::Integer { default, min, max },
        };
        assert!(one(int(5, 1, 10)).is_ok());
        assert!(one(int(0, 1, 10)).is_err());
        assert!(one(int(5, 10, 1)).is_err());
        let named = |key: &str| Entry {
            key: key.into(),
            description: String::new(),
            spec: Spec::Boolean(true),
        };
        assert!(one(named("Bad Key")).is_err());
        assert!(one(named("")).is_err());
        assert!(
            Config::default()
                .declare(vec![named("a"), named("a")], None)
                .is_err()
        );
        let choice = |default: &str, options: &[&str]| Entry {
            key: "c".into(),
            description: String::new(),
            spec: Spec::Choice {
                default: default.into(),
                options: options.iter().map(|o| (*o).to_owned()).collect(),
            },
        };
        assert!(one(choice("a", &["a", "b"])).is_ok());
        assert!(one(choice("z", &["a", "b"])).is_err());
        assert!(one(choice("a", &[])).is_err());
        let nan = Entry {
            key: "f".into(),
            description: String::new(),
            spec: Spec::Number {
                default: f64::NAN,
                min: 0.0,
                max: 1.0,
            },
        };
        assert!(one(nan).is_err());
        let many: Vec<Entry> = (0..=MAX_ENTRIES).map(|i| named(&format!("k{i}"))).collect();
        assert!(Config::default().declare(many, None).is_err());
    }
}
