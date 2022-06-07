use bigdecimal::BigDecimal;
use std::cmp::Ordering;

pub fn motion_label(power: &BigDecimal) -> String {
  let one = BigDecimal::from(1);
  match power.cmp(&one) {
    Ordering::Greater => String::from("Supermotion"),
    Ordering::Less => String::from("Submotion"),
    Ordering::Equal => String::from("Simple motion"),
  }
}