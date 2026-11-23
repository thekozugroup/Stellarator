//! The number stepper and the common-seqs row (frontend/src/components/runs/number-stepper.tsx,
//! seqs-choices.tsx), shared by New run and the Run screen.
//!
//! - [`number_stepper`]: a whole-number field with a minus and a plus button beside it, both in the
//!   raised (secondary) tone. The value stays text so typing is never blocked; the buttons clamp to
//!   the range and disable at its ends. Up and Down in the field step the value, as in a number
//!   input.
//! - [`seqs_choices`]: the common seqs values ([`SEQS_CHOICES`]) as one segmented control that never
//!   wraps: from 768 px it is as wide as its segments, below that it fills its column.

use std::rc::Rc;

use nsh_core::prelude::*;

use super::button::{Button, Size, Variant};
use super::choice::segmented_with_get;
use super::field::text_field;
use super::icon;
use super::page::is_phone;

/// Common seqs values, offered as one segmented row on New run and on a run.
pub const SEQS_CHOICES: [i64; 8] = [1, 4, 8, 16, 32, 64, 128, 256];

/// The labels of [`SEQS_CHOICES`].
const SEQS_LABELS: [&str; 8] = ["1", "4", "8", "16", "32", "64", "128", "256"];

/// The whole number at the start of `s` (JavaScript's `parseInt(s, 10)`): `None` when it does not
/// start with one.
pub fn parse_int(s: &str) -> Option<i64> {
    let s = s.trim_start();
    let (sign, rest) = match s.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, s.strip_prefix('+').unwrap_or(s)),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse::<i64>().ok().map(|n| sign * n)
}

/// `value` moved by `delta` and held to `min..=max`, as text. A value that is not a number counts
/// as `min`.
pub fn stepped(value: &str, delta: i64, min: i64, max: i64) -> String {
    let current = parse_int(value).unwrap_or(min);
    (current + delta).clamp(min, max).to_string()
}

/// A number stepper over `value`, from `min` to `max`; `name` says what it counts for the buttons'
/// names ("Decrease seqs").
pub struct NumberStepper {
    value: Signal<String>,
    name: SharedString,
    min: i64,
    max: i64,
    invalid: Option<Rc<dyn Fn() -> bool>>,
    on_change: Option<Rc<dyn Fn(&str)>>,
    on_blur: Option<Rc<dyn Fn()>>,
    on_submit: Option<Rc<dyn Fn()>>,
    element: Option<ElementRef>,
}

pub fn number_stepper(value: Signal<String>, name: impl Into<SharedString>, min: i64, max: i64) -> NumberStepper {
    NumberStepper { value, name: name.into(), min, max, invalid: None, on_change: None, on_blur: None, on_submit: None, element: None }
}

impl NumberStepper {
    /// The red edge and ring while `invalid` holds.
    pub fn invalid(mut self, invalid: impl Fn() -> bool + 'static) -> Self {
        self.invalid = Some(Rc::new(invalid));
        self
    }

    /// Runs after each change: typing or a button.
    pub fn on_change(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Runs when the field loses the focus.
    pub fn on_blur(mut self, f: impl Fn() + 'static) -> Self {
        self.on_blur = Some(Rc::new(f));
        self
    }

    /// Return in the field.
    pub fn on_submit(mut self, f: impl Fn() + 'static) -> Self {
        self.on_submit = Some(Rc::new(f));
        self
    }

    /// The field's element, to focus it.
    pub fn element(mut self, element: &ElementRef) -> Self {
        self.element = Some(*element);
        self
    }
}

impl IntoElement for NumberStepper {
    fn into_element(self) -> AnyElement {
        let NumberStepper { value, name, min, max, invalid, on_change, on_blur, on_submit, element } = self;
        let step = {
            let on_change = on_change.clone();
            Rc::new(move |delta: i64| {
                let next = stepped(&value.get_untracked(), delta, min, max);
                value.set(next.clone());
                if let Some(f) = &on_change {
                    f(&next);
                }
            })
        };
        let current = move || parse_int(&value.get()).unwrap_or(min);

        let mut field = text_field(value).numeric().centered().width(80.0);
        if let Some(i) = invalid {
            field = field.invalid(move || i());
        }
        if let Some(f) = on_change {
            field = field.on_change(move |t| f(t));
        }
        if let Some(f) = on_blur {
            field = field.on_blur(move || f());
        }
        if let Some(f) = on_submit {
            field = field.on_submit(move || f());
        }
        if let Some(e) = element {
            field = field.element(&e);
        }

        let (up, down) = (step.clone(), step.clone());
        let (minus, plus) = (step.clone(), step);
        view()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .child(
                Button::icon(icon::MINUS, format!("Decrease {name}"))
                    .variant(Variant::Secondary)
                    .size(Size::Icon)
                    .without_tooltip()
                    .disabled(move || current() <= min)
                    .on_press(move || minus(-1)),
            )
            // Up and Down step the value, as in any number input.
            .child(view().on_key_down(move |e: &KeyEvent| match &*e.key {
                "ArrowUp" => up(1),
                "ArrowDown" => down(-1),
                _ => {}
            }).child(field))
            .child(
                Button::icon(icon::PLUS, format!("Increase {name}"))
                    .variant(Variant::Secondary)
                    .size(Size::Icon)
                    .without_tooltip()
                    .disabled(move || current() >= max)
                    .on_press(move || plus(1)),
            )
            .into_element()
    }
}

/// The common seqs values as one segmented control: `value` is the current number (none of the
/// segments is chosen when it is not one of them), `on_change` hears a choice.
pub fn seqs_choices(value: impl Fn() -> i64 + 'static, on_change: impl Fn(i64) + 'static) -> impl IntoElement {
    let options: Vec<(i64, &'static str)> = SEQS_CHOICES.iter().copied().zip(SEQS_LABELS).collect();
    segmented_with_get(
        options,
        move || {
            let n = value();
            if SEQS_CHOICES.contains(&n) { n } else { -1 }
        },
        on_change,
        "Common seqs values",
        // Below 768 px the segments share the row; above it the control is as wide as they are.
        untracked(is_phone),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_int_reads_the_leading_whole_number() {
        assert_eq!(parse_int("16"), Some(16));
        assert_eq!(parse_int("  16abc"), Some(16));
        assert_eq!(parse_int("-3"), Some(-3));
        assert_eq!(parse_int("+7"), Some(7));
        assert_eq!(parse_int("3.9"), Some(3));
        assert_eq!(parse_int(""), None);
        assert_eq!(parse_int("abc"), None);
        assert_eq!(parse_int("-"), None);
    }

    #[test]
    fn the_buttons_clamp_to_the_range() {
        assert_eq!(stepped("16", 1, 1, 4096), "17");
        assert_eq!(stepped("1", -1, 1, 4096), "1");
        assert_eq!(stepped("4096", 1, 1, 4096), "4096");
        // An empty or invalid field counts as the minimum.
        assert_eq!(stepped("", 1, 1, 4096), "2");
        assert_eq!(stepped("x", -1, 1, 4096), "1");
        // A value out of range comes back in range.
        assert_eq!(stepped("9000", -1, 1, 4096), "4096");
    }
}
