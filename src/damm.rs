// Based on https://en.wikipedia.org/wiki/Damm_algorithm

const OPERATION_TABLE:[u8; 100] = [
    0, 3, 1, 7, 5, 9, 8, 6, 4, 2,
    7, 0, 9, 2, 1, 5, 4, 8, 6, 3,
    4, 2, 0, 6, 8, 7, 1, 3, 5, 9,
    1, 7, 5, 0, 9, 8, 3, 4, 2, 6,
    6, 1, 2, 3, 0, 4, 5, 9, 7, 8,
    3, 6, 7, 4, 2, 0, 9, 5, 8, 1,
    5, 8, 6, 9, 7, 2, 0, 1, 3, 4,
    8, 9, 4, 5, 3, 6, 2, 0, 1, 7,
    9, 4, 3, 8, 6, 1, 7, 2, 0, 5,
    2, 5, 8, 1, 4, 3, 6, 7, 9, 0,
];

fn operation(top:usize, left:usize) -> u8 {
    OPERATION_TABLE[left * 10 + top]
}

/// Given a slice of ASCII digits, what should the check digit be (single ASCII char)
/// Panics on non-ascii char in input
pub fn check_digit(input:&[u8]) -> u8 {
    let mut res = 0u8;
    for digit in input {
        if *digit > 9 {
            panic!("All digits must be between 0 and 9 inclusive");
        }
        res = operation(*digit as usize, res as usize);
    }
    res
}

/// Given a string-like, return a string with the check digit appended
pub fn add_to_str<S: Into<String>>(s:S) -> String {
    let mut strr:String = s.into();
    let mut digits:Vec<u8> = Vec::new();
    for c in strr.chars() {
        if ('0'..='9').contains(&c) {
            digits.push(((c as u32) - ('0' as u32)) as u8);
        }else{
            panic!("invalid digit");
        }
    }
    let cd = check_digit(&digits);
    strr.push((b'0' + (cd as u8)) as char);
    strr
}

/// Validates that a string is a valid damm number.
/// If the number is valid, returns Some(Vec<u8>), where the vec is the digits (0-9, NOT ascii) sans check digit
/// If the number is invalid, returns None
pub fn validate(s:&str) -> Option<Vec<u8>> {
    let mut digits:Vec<u8> = Vec::with_capacity(s.len());
    for c in s.chars() {
        if ('0'..='9').contains(&c) {
            digits.push(((c as u32) - ('0' as u32)) as u8);
        }else{
            return None;
        }
    }
    if check_digit(&digits) == 0 {
        digits.pop();
        Some(digits)
    } else {
        None
    }
}

pub fn validate_ascii(s:&str) -> Option<Vec<u8>> {
    let mut low_digits:Vec<u8> = Vec::with_capacity(s.len());
    let mut ascii_digits:Vec<u8> = Vec::with_capacity(s.len());
    for c in s.chars() {
        if ('0'..='9').contains(&c) {
            low_digits.push(((c as u32) - ('0' as u32)) as u8);
            ascii_digits.push(c as u8);
        }else{
            return None;
        }
    }
    if check_digit(&low_digits) == 0 {
        ascii_digits.pop();
        Some(ascii_digits)
    } else {
        None
    }
}