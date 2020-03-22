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

pub fn check_digit(input:&[u8]) -> u8 {
    let mut res = 0u8;
    for digit in input {
        if *digit > 9 {
            panic!("All digits must be between 0 and 9 inclusive");
        }
        res = operation(*digit as usize, res as usize);
    }
    return res;
}

pub fn add_to_str<S: Into<String>>(s:S) -> String {
    let mut strr:String = s.into();
    let mut digits:Vec<u8> = Vec::new();
    for c in strr.chars() {
        if '0' <= c && c <= '9' {
            digits.push(((c as u32) - ('0' as u32)) as u8);
        }else{
            panic!("invalid digit");
        }
    }
    let cd = check_digit(&digits);
    strr.push((('0' as u8) + (cd as u8)) as char);
    return strr;
}

pub fn validate(s:&str) -> Option<Vec<u8>> {
    let mut digits:Vec<u8> = Vec::with_capacity(s.len());
    for c in s.chars() {
        if '0' <= c && c <= '9' {
            digits.push(((c as u32) - ('0' as u32)) as u8);
        }else{
            return None;
        }
    }
    if check_digit(&digits) == 0 {
        digits.pop();
        return Some(digits);
    } else {
        return None;
    }
}
