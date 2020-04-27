pub fn is_win(yes_votes:i64, no_votes:i64, is_super:bool) -> bool {
    if is_super {
        return (yes_votes/2, yes_votes%2) > (no_votes, 0);
    } else {
        return yes_votes > no_votes;
    }
}

#[cfg(test)]
mod test {
    fn old_is_win(yes_votes:i64, no_votes:i64, is_super:bool) -> bool {
      if is_super {
          let total = yes_votes + no_votes;
          let div = total / 3;
          let rem = total % 3;
          // 10 = 3 rem 1
          // win is >= 7 (div*2+rem)
          // 11 = 3 rem 2
          // win is >= 7 (div*2+rem)
          // 12 = 4 rem 0
          // 8 is a "tie", so lose
          // win is >= 9 (div*2+rem)+1
          let winning_amount = (div*2+rem) + if rem == 0 {1} else {0};
          return yes_votes >= winning_amount;
      }else{
          return yes_votes > no_votes;
      }
    }

    #[test]
    fn wins_match(){
        use super::is_win;
        assert_eq!(old_is_win(1, 0, false), is_win(1, 0, false));
        assert_eq!(old_is_win(1, 1, false), is_win(1, 1, false));
        assert_eq!(old_is_win(1, 2, false), is_win(1, 2, false));
        assert_eq!(old_is_win(2, 1, true),  is_win(2, 1, true));
        assert_eq!(old_is_win(3, 1, true),  is_win(3, 1, true));
    }
}