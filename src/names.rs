pub static KNOWN_NAMES: phf::Map<u64, &'static str> = phf::phf_map! {
    0u64 => "TheOtherPitOfFire",
    1u64 => "PitOfFire",
    2u64 => "Dos",
    3u64 => "Three",
    4u64 => "Four",
    125003180219170816u64 => "Colin",
    155438323354042368u64 => "Ben",
    165858230327574528u64 => "Shelvacu",
    175691653770641409u64 => "DDR",
    173650493145350145u64 => "Sparks",
    182663630280589312u64 => "Azure",
    189163550890000384u64 => "hemaglox",
    189620154122895360u64 => "Leeli",
    240939050360504320u64 => "InvisiBrony",
    265905786469744640u64 => "AtomicTaco",
    271540455584301057u64 => "Anthony",
    308798067023544320u64 => "Razgriz",
    359950109229973504u64 => "ManganeseFrog",
    368635452925935616u64 => "TopHatimus",
    373610438560317441u64 => "Matt",
};

pub fn name_of(user:impl Into<crate::models::UserId>) -> std::borrow::Cow<'static, str> {
    let u:crate::models::UserId = user.into();
    trace!("name_of");
    if let Some(name) = KNOWN_NAMES.get(&u.into_u64()) {
        (*name).into()
    } else {
        u.to_string().into()
    }
}