use bigdecimal::BigDecimal;

pub fn is_win(yes_votes: i64, no_votes: i64, power: &BigDecimal) -> bool {
    BigDecimal::from(no_votes) * power < BigDecimal::from(yes_votes)
}

#[cfg(test)]
mod test {
    #[test]
    fn does_win() {
        use super::is_win;
        use bigdecimal::BigDecimal;

        // Simple majority
        let one = BigDecimal::from(1);
        assert_eq!(is_win(1, 0, &one), true);
        assert_eq!(is_win(1, 1, &one), false);
        assert_eq!(is_win(1, 2, &one), false);
        assert_eq!(is_win(2, 1, &one), true);
        assert_eq!(is_win(i64::MAX, i64::MAX - 1, &one), true);
        assert_eq!(is_win(i64::MAX, i64::MAX, &one), false);

        // Two-thirds supermajority
        let two = BigDecimal::from(2);
        assert_eq!(is_win(2, 1, &two), false);
        assert_eq!(is_win(3, 1, &two), true);
        assert_eq!(is_win(3, 2, &two), false);
        assert_eq!(is_win(i64::MAX, i64::MAX / 2, &two), true); // slightly less than one-half, due to rounding
        assert_eq!(is_win(i64::MAX, i64::MAX / 2 + 1, &two), false);

        // One-third submajority
        let half = BigDecimal::from(0.5);
        assert_eq!(is_win(1, 1, &half), true);
        assert_eq!(is_win(2, 3, &half), true);
        assert_eq!(is_win(2, 4, &half), false);
        assert_eq!(is_win(i64::MAX / 2, i64::MAX, &half), false); // slightly less than one-half, due to rounding
        assert_eq!(is_win(i64::MAX / 2 + 1, i64::MAX, &half), true);

        // Extreme cases
        assert_eq!(is_win(1, 3, &BigDecimal::from(1.0 / 3.0)), true); // slighty less than one-third
        assert_eq!(
            is_win(1, 3, &BigDecimal::from((1.0 / 3.0) + f64::EPSILON)), // slightly more than one-third
            false
        );
        assert_eq!(is_win(i64::MAX, 1, &BigDecimal::from(i64::MAX)), false);
        assert_eq!(is_win(i64::MAX, 1, &BigDecimal::from(i64::MAX - 1)), true);
    }
}
