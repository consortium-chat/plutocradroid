use bigdecimal::BigDecimal;

pub fn is_win(yes_votes: i64, no_votes: i64, power: &BigDecimal) -> bool {
    BigDecimal::from(no_votes) * power >= BigDecimal::from(yes_votes)
}

#[cfg(test)]
mod test {
    #[test]
    fn does_win(){
        use super::is_win;
        use bigdecimal::BigDecimal;
        let one = BigDecimal::from(1);
        let two = BigDecimal::from(2);
        let half = BigDecimal::from(0.5);
        let thirdish = BigDecimal::from(0.3333333333);
        let tenth = BigDecimal::from(0.1);
        assert_eq!(is_win(1, 0, &one), true);
        assert_eq!(is_win(1, 1, &one), false);
        assert_eq!(is_win(1, 2, &one), false);
        assert_eq!(is_win(2, 1, &one), true);
        assert_eq!(is_win(2, 1, &two), false);
        assert_eq!(is_win(3, 1, &two), true);
        assert_eq!(is_win(1, 1, &half), true);
        assert_eq!(is_win(1, 3, &thirdish), true);
        assert_eq!(is_win(100, 1000, &tenth), false);
        assert_eq!(is_win(101, 1000, &tenth), true);
    }
}