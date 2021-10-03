use std::convert::TryInto;
use crate::error;

pub fn now_1123() -> String {
    let now_2822 = chrono::Utc::now().to_rfc2822();
    let mut now_1123 = now_2822[..(now_2822.len()-5)].to_string();
    now_1123.push_str("GMT");
    now_1123
}

pub fn parse_date_1123(s: &str) -> Result<chrono::DateTime<chrono::offset::Utc>, error::Error> {
    let (_, s) = parse_wkday(s)?;
    let s = discard_char(',', s)?;
    let s = discard_char(' ', s)?;
    let (date, s) = parse_date(s)?;
    let s = discard_char(' ', s)?;
    let (time, s) = parse_time(s)?;
    let s = discard_char(' ', s)?;
    discard_string("GMT", s)?;

    Ok(chrono::DateTime::from_utc(date.and_time(time), chrono::offset::Utc))
}

fn parse_wkday(s: &str) -> Result<(chrono::Weekday, &str), error::Error> {
    let rest = &s[3..];
    match &s[..3] {
        "Mon" => Ok((chrono::Weekday::Mon, rest)),
        "Tue" => Ok((chrono::Weekday::Tue, rest)),
        "Wed" => Ok((chrono::Weekday::Wed, rest)),
        "Thu" => Ok((chrono::Weekday::Thu, rest)),
        "Fri" => Ok((chrono::Weekday::Fri, rest)),
        "Sat" => Ok((chrono::Weekday::Sat, rest)),
        "Sun" => Ok((chrono::Weekday::Sun, rest)),
        _ => Err(error::Error::new("Could not parse weekday".to_string())),
    }
}

fn parse_date(s: &str) -> Result<(chrono::naive::NaiveDate, &str), error::Error> {
    let (day, s) = parse_digits(2, s)?;
    let s = discard_char(' ', s)?;
    let (month, s) = parse_month(s)?;
    let s = discard_char(' ', s)?;
    let (year, s) = parse_digits(4, s)?;

    Ok((chrono::NaiveDate::from_ymd(year, month, day.try_into().unwrap()), s))
}

fn parse_month(s: &str) -> Result<(u32, &str), error::Error> {
    let rest = &s[3..];
    match &s[..3] {
        "Jan" => Ok((1, rest)),
        "Feb" => Ok((2, rest)),
        "Mar" => Ok((3, rest)),
        "Apr" => Ok((4, rest)),
        "May" => Ok((5, rest)),
        "Jun" => Ok((6, rest)),
        "Jul" => Ok((7, rest)),
        "Aug" => Ok((8, rest)),
        "Sep" => Ok((9, rest)),
        "Oct" => Ok((10, rest)),
        "Nov" => Ok((11, rest)),
        "Dec" => Ok((12, rest)),
        _ => Err(error::Error::new("Could not parse month".to_string())),
    }
}

fn parse_time(s: &str) -> Result<(chrono::naive::NaiveTime, &str), error::Error> {
    let (hour, s) = parse_digits(2, s)?;
    let s = discard_char(':', s)?;
    let (minute, s) = parse_digits(2, s)?;
    let s = discard_char(':', s)?;
    let (second, s) = parse_digits(2, s)?;

    Ok((chrono::naive::NaiveTime::from_hms(hour.try_into().unwrap(), minute.try_into().unwrap(), second.try_into().unwrap()), s))
}

fn parse_digits(n: u32, s: &str) -> Result<(i32, &str), error::Error> {
    let mut running = 0;
    let mut s_local = s;
    let mut d;
    for i in 0..n {
        let parsed_digits = parse_digit(s_local)?;
        d = parsed_digits.0;
        s_local = parsed_digits.1;
        running += d * (10i32).pow(n - i - 1);
    }
    Ok((running, s_local))
}

fn parse_digit(s: &str) -> Result<(i32, &str), error::Error> {
    let rest = &s[1..];
    match &s[..1] {
        "0" => Ok((0, rest)),
        "1" => Ok((1, rest)),
        "2" => Ok((2, rest)),
        "3" => Ok((3, rest)),
        "4" => Ok((4, rest)),
        "5" => Ok((5, rest)),
        "6" => Ok((6, rest)),
        "7" => Ok((7, rest)),
        "8" => Ok((8, rest)),
        "9" => Ok((9, rest)),
        _ => Err(error::Error::new("Could not parse digit".to_string())),
    }
}

fn discard_string<'a>(to_find: &str, s: &'a str) -> Result<&'a str, error::Error> {
    if s.starts_with(to_find) {
        Ok(&s[to_find.len()..])
    } else {
        Err(error::Error::new(format!("Could not parse '{}'", to_find)))
    }
}

fn discard_char(to_find: char, s: &str) -> Result<&str, error::Error> {
    if s.starts_with(to_find) {
        Ok(&s[1..])
    } else {
        Err(error::Error::new(format!("Could not parse '{}'", to_find)))
    }
}
