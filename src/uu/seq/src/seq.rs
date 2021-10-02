// TODO: Make -w flag work with decimals
// TODO: Support -f flag

// spell-checker:ignore (ToDO) istr chiter argptr ilen

#[macro_use]
extern crate uucore;

use clap::{crate_version, App, AppSettings, Arg};
use num_bigint::BigInt;
use num_traits::One;
use num_traits::Zero;
use num_traits::{Num, ToPrimitive};
use std::cmp;
use std::io::{stdout, ErrorKind, Write};
use std::str::FromStr;

mod digits;
use crate::digits::num_fractional_digits;
use crate::digits::num_integral_digits;

use uucore::display::Quotable;

static ABOUT: &str = "Display numbers from FIRST to LAST, in steps of INCREMENT.";
static OPT_SEPARATOR: &str = "separator";
static OPT_TERMINATOR: &str = "terminator";
static OPT_WIDTHS: &str = "widths";

static ARG_NUMBERS: &str = "numbers";

fn usage() -> String {
    format!(
        "{0} [OPTION]... LAST
    {0} [OPTION]... FIRST LAST
    {0} [OPTION]... FIRST INCREMENT LAST",
        uucore::execution_phrase()
    )
}
#[derive(Clone)]
struct SeqOptions {
    separator: String,
    terminator: String,
    widths: bool,
}

enum Number {
    /// Negative zero, as if it were an integer.
    MinusZero,
    BigInt(BigInt),
    F64(f64),
}

impl Number {
    fn is_zero(&self) -> bool {
        match self {
            Number::MinusZero => true,
            Number::BigInt(n) => n.is_zero(),
            Number::F64(n) => n.is_zero(),
        }
    }

    fn into_f64(self) -> f64 {
        match self {
            Number::MinusZero => -0.,
            // BigInt::to_f64() can not return None.
            Number::BigInt(n) => n.to_f64().unwrap(),
            Number::F64(n) => n,
        }
    }

    /// Convert this number into a bigint, consuming it.
    ///
    /// For floats, this returns the [`BigInt`] corresponding to the
    /// floor of the number.
    fn into_bigint(self) -> BigInt {
        match self {
            Number::MinusZero => BigInt::zero(),
            Number::F64(x) => BigInt::from(x.floor() as i64),
            Number::BigInt(n) => n,
        }
    }
}

impl FromStr for Number {
    type Err = String;
    fn from_str(mut s: &str) -> Result<Self, Self::Err> {
        s = s.trim_start();
        if s.starts_with('+') {
            s = &s[1..];
        }

        match s.parse::<BigInt>() {
            Ok(n) => {
                // If `s` is '-0', then `parse()` returns
                // `BigInt::zero()`, but we need to return
                // `Number::MinusZero` instead.
                if n == BigInt::zero() && s.starts_with('-') {
                    Ok(Number::MinusZero)
                } else {
                    Ok(Number::BigInt(n))
                }
            }
            Err(_) => match s.parse::<f64>() {
                Ok(value) if value.is_nan() => Err(format!(
                    "invalid 'not-a-number' argument: {}\nTry '{} --help' for more information.",
                    s.quote(),
                    uucore::execution_phrase(),
                )),
                Ok(value) => Ok(Number::F64(value)),
                Err(_) => Err(format!(
                    "invalid floating point argument: {}\nTry '{} --help' for more information.",
                    s.quote(),
                    uucore::execution_phrase(),
                )),
            },
        }
    }
}

/// A range of integers.
///
/// The elements are (first, increment, last).
type RangeInt = (BigInt, BigInt, BigInt);

/// A range of f64.
///
/// The elements are (first, increment, last).
type RangeF64 = (f64, f64, f64);

pub fn uumain(args: impl uucore::Args) -> i32 {
    let usage = usage();
    let matches = uu_app().usage(&usage[..]).get_matches_from(args);

    let numbers = matches.values_of(ARG_NUMBERS).unwrap().collect::<Vec<_>>();

    let options = SeqOptions {
        separator: matches.value_of(OPT_SEPARATOR).unwrap_or("\n").to_string(),
        terminator: matches.value_of(OPT_TERMINATOR).unwrap_or("\n").to_string(),
        widths: matches.is_present(OPT_WIDTHS),
    };

    let mut largest_dec = 0;
    let mut padding = 0;
    let first = if numbers.len() > 1 {
        let slice = numbers[0];
        largest_dec = num_fractional_digits(slice).unwrap_or_else(|_| {
            crash!(
                1,
                "invalid floating point argument: {}\n Try '{} --help' for more information.",
                slice.quote(),
                uucore::execution_phrase()
            )
        });
        padding = num_integral_digits(slice).unwrap_or_else(|_| {
            crash!(
                1,
                "invalid floating point argument: {}\n Try '{} --help' for more information.",
                slice.quote(),
                uucore::execution_phrase()
            )
        });
        crash_if_err!(1, slice.parse())
    } else {
        Number::BigInt(BigInt::one())
    };
    let increment = if numbers.len() > 2 {
        let slice = numbers[1];
        let dec = num_fractional_digits(slice).unwrap_or_else(|_| {
            crash!(
                1,
                "invalid floating point argument: {}\n Try '{} --help' for more information.",
                slice.quote(),
                uucore::execution_phrase()
            )
        });
        let int_digits = num_integral_digits(slice).unwrap_or_else(|_| {
            crash!(
                1,
                "invalid floating point argument: {}\n Try '{} --help' for more information.",
                slice.quote(),
                uucore::execution_phrase()
            )
        });
        largest_dec = cmp::max(largest_dec, dec);
        padding = cmp::max(padding, int_digits);
        crash_if_err!(1, slice.parse())
    } else {
        Number::BigInt(BigInt::one())
    };
    if increment.is_zero() {
        show_error!(
            "invalid Zero increment value: '{}'\nTry '{} --help' for more information.",
            numbers[1],
            uucore::execution_phrase()
        );
        return 1;
    }
    let last: Number = {
        let slice = numbers[numbers.len() - 1];
        let int_digits = num_integral_digits(slice).unwrap_or_else(|_| {
            crash!(
                1,
                "invalid floating point argument: {}\n Try '{} --help' for more information.",
                slice.quote(),
                uucore::execution_phrase()
            )
        });
        padding = cmp::max(padding, int_digits);
        crash_if_err!(1, slice.parse())
    };

    let is_negative_zero_f64 = |x: f64| x == -0.0 && x.is_sign_negative() && largest_dec == 0;
    let result = match (first, last, increment) {
        // For example, `seq -0 1 2` or `seq -0 1 2.0`.
        (Number::MinusZero, last, Number::BigInt(increment)) => print_seq_integers(
            (BigInt::zero(), increment, last.into_bigint()),
            options.separator,
            options.terminator,
            options.widths,
            padding,
            true,
        ),
        // For example, `seq -0e0 1 2` or `seq -0e0 1 2.0`.
        (Number::F64(x), last, Number::BigInt(increment)) if is_negative_zero_f64(x) => {
            print_seq_integers(
                (BigInt::zero(), increment, last.into_bigint()),
                options.separator,
                options.terminator,
                options.widths,
                padding,
                true,
            )
        }
        // For example, `seq 0 1 2` or `seq 0 1 2.0`.
        (Number::BigInt(first), last, Number::BigInt(increment)) => print_seq_integers(
            (first, increment, last.into_bigint()),
            options.separator,
            options.terminator,
            options.widths,
            padding,
            false,
        ),
        // For example, `seq 0 0.5 1` or `seq 0.0 0.5 1` or `seq 0.0 0.5 1.0`.
        (first, last, increment) => print_seq(
            (first.into_f64(), increment.into_f64(), last.into_f64()),
            largest_dec,
            options.separator,
            options.terminator,
            options.widths,
            padding,
        ),
    };
    match result {
        Ok(_) => 0,
        Err(err) if err.kind() == ErrorKind::BrokenPipe => 0,
        Err(_) => 1,
    }
}

pub fn uu_app() -> App<'static, 'static> {
    App::new(uucore::util_name())
        .setting(AppSettings::AllowLeadingHyphen)
        .version(crate_version!())
        .about(ABOUT)
        .arg(
            Arg::with_name(OPT_SEPARATOR)
                .short("s")
                .long("separator")
                .help("Separator character (defaults to \\n)")
                .takes_value(true)
                .number_of_values(1),
        )
        .arg(
            Arg::with_name(OPT_TERMINATOR)
                .short("t")
                .long("terminator")
                .help("Terminator character (defaults to \\n)")
                .takes_value(true)
                .number_of_values(1),
        )
        .arg(
            Arg::with_name(OPT_WIDTHS)
                .short("w")
                .long("widths")
                .help("Equalize widths of all numbers by padding with zeros"),
        )
        .arg(
            Arg::with_name(ARG_NUMBERS)
                .multiple(true)
                .takes_value(true)
                .allow_hyphen_values(true)
                .max_values(3)
                .required(true),
        )
}

fn done_printing<T: Num + PartialOrd>(next: &T, increment: &T, last: &T) -> bool {
    if increment >= &T::zero() {
        next > last
    } else {
        next < last
    }
}

/// Floating point based code path
fn print_seq(
    range: RangeF64,
    largest_dec: usize,
    separator: String,
    terminator: String,
    pad: bool,
    padding: usize,
) -> std::io::Result<()> {
    let stdout = stdout();
    let mut stdout = stdout.lock();
    let (first, increment, last) = range;
    let mut i = 0isize;
    let is_first_minus_zero = first == -0.0 && first.is_sign_negative();
    let mut value = first + i as f64 * increment;
    let padding = if pad { padding + 1 + largest_dec } else { 0 };
    let mut is_first_iteration = true;
    while !done_printing(&value, &increment, &last) {
        if !is_first_iteration {
            write!(stdout, "{}", separator)?;
        }
        let mut width = padding;
        if is_first_iteration && is_first_minus_zero {
            write!(stdout, "-")?;
            width -= 1;
        }
        is_first_iteration = false;
        write!(
            stdout,
            "{value:>0width$.precision$}",
            value = value,
            width = width,
            precision = largest_dec,
        )?;
        i += 1;
        value = first + i as f64 * increment;
    }
    if !is_first_iteration {
        write!(stdout, "{}", terminator)?;
    }
    stdout.flush()?;
    Ok(())
}

/// Print an integer sequence.
///
/// This function prints a sequence of integers defined by `range`,
/// which defines the first integer, last integer, and increment of the
/// range. The `separator` is inserted between each integer and
/// `terminator` is inserted at the end.
///
/// The `pad` parameter indicates whether to pad numbers to the width
/// given in `padding`.
///
/// If `is_first_minus_zero` is `true`, then the `first` parameter is
/// printed as if it were negative zero, even though no such number
/// exists as an integer (negative zero only exists for floating point
/// numbers). Only set this to `true` if `first` is actually zero.
fn print_seq_integers(
    range: RangeInt,
    separator: String,
    terminator: String,
    pad: bool,
    padding: usize,
    is_first_minus_zero: bool,
) -> std::io::Result<()> {
    let stdout = stdout();
    let mut stdout = stdout.lock();
    let (first, increment, last) = range;
    let mut value = first;
    let mut is_first_iteration = true;
    while !done_printing(&value, &increment, &last) {
        if !is_first_iteration {
            write!(stdout, "{}", separator)?;
        }
        let mut width = padding;
        if is_first_iteration && is_first_minus_zero {
            write!(stdout, "-")?;
            width -= 1;
        }
        is_first_iteration = false;
        if pad {
            write!(stdout, "{number:>0width$}", number = value, width = width)?;
        } else {
            write!(stdout, "{}", value)?;
        }
        value += &increment;
    }

    if !is_first_iteration {
        write!(stdout, "{}", terminator)?;
    }
    Ok(())
}
