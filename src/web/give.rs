use crate::models::UserId;

#[derive(Debug, Clone)]
struct GiveDestination {
    expected_name: Option<String>,
    id: UserId,
}

use rocket::http::RawStr;
use std::str::FromStr;
impl<'v> rocket::request::FromFormValue<'v> for GiveDestination {
    type Error = &'v RawStr;

    fn from_form_value(form_value: &'v RawStr) -> Result<Self, Self::Error> {
        let s = <String as rocket::request::FromFormValue>::from_form_value(form_value)?;
        dbg!(&s);
        let c = match crate::GIVE_DESTINATION_RE.captures(s.as_str()) {
            Some(v) => v,
            None => return Err(form_value),
        };
        dbg!(&c);
        let expected_name = c.get(1).map(|v| v.as_str().to_string());
        let id_str = c.get(2).ok_or(form_value)?;
        let id:UserId = i64::from_str(id_str.as_str()).map_err(|_| form_value)?.try_into().map_err(|_| form_value)?;
        dbg!();
        Ok(Self{expected_name, id})
    }
}

#[derive(Debug, Clone, FromForm)]
struct GiveForm {
    csrf: String,
    quantity: i64,
    ty: String,
    destination: GiveDestination,
}
