use candid::Principal;

use crate::poker::{
    core::{Card, Suit, Value},
    game::{
        table_functions::{
            table::{Table, TableId},
            tests::{create_user, get_table_config},
            types::BetType,
        },
        types::GameType,
        utils::convert_to_e8s,
    },
};

// ========== PLO4 Tests ==========

#[test]
fn test_plo4_deals_four_hole_cards() {
    // Test that PLO4 deals exactly 4 hole cards to each player
    let mut table = Table::new(
        TableId(Principal::anonymous()),
        get_table_config(GameType::PotLimitOmaha4(convert_to_e8s(1.0)), 2),
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    );
    let user1 = create_user(
        Principal::from_text("2chl6-4hpzw-vqaaa-aaaaa-c").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );
    let user2 = create_user(
        Principal::from_text("br5f7-7uaaa-aaaaa-qaaca-cai").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );

    assert!(table.add_user(user1.clone(), 0, false).is_ok());
    assert!(table.add_user(user2.clone(), 1, false).is_ok());

    assert_eq!(
        table.start_betting_round(vec![0, 1, 2, 3, 4, 5, 6, 7, 8]),
        Ok((Vec::new(), Vec::new()))
    );

    // Check that each player has exactly 4 hole cards
    assert_eq!(
        table
            .user_table_data
            .get(&user1.principal_id)
            .unwrap()
            .cards
            .len(),
        4
    );
    assert_eq!(
        table
            .user_table_data
            .get(&user2.principal_id)
            .unwrap()
            .cards
            .len(),
        4
    );
}

#[test]
fn test_plo4_hand_evaluation_exactly_two_hole_three_community() {
    // Test that PLO4 correctly evaluates hands using exactly 2 hole + 3 community
    let mut table = Table::new(
        TableId(Principal::anonymous()),
        get_table_config(GameType::PotLimitOmaha4(convert_to_e8s(1.0)), 2),
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    );
    let user1 = create_user(
        Principal::from_text("2chl6-4hpzw-vqaaa-aaaaa-c").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );
    let user2 = create_user(
        Principal::from_text("br5f7-7uaaa-aaaaa-qaaca-cai").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );

    assert!(table.add_user(user1.clone(), 0, false).is_ok());
    assert!(table.add_user(user2.clone(), 1, false).is_ok());

    let player1_uid = table.get_player_at_seat(0).unwrap();
    let player2_uid = table.get_player_at_seat(1).unwrap();

    // Start betting round
    assert_eq!(table.start_betting_round(vec![0, 1]), Ok((vec![], vec![])));

    // Both players check to showdown
    assert_eq!(table.bet(player1_uid, BetType::Called), Ok(()));
    assert_eq!(table.bet(player2_uid, BetType::Called), Ok(()));

    // Set community cards: [K♦, K♠, 9♥, 5♣, 3♦]
    table.community_cards = vec![
        Card::new(Value::King, Suit::Diamond),
        Card::new(Value::King, Suit::Spade),
        Card::new(Value::Nine, Suit::Heart),
        Card::new(Value::Five, Suit::Club),
        Card::new(Value::Three, Suit::Diamond),
    ];

    // Player 1: [A♠, A♥, 2♠, 3♠]
    // Best hand: Use A♠, A♥ from hole + K♦, K♠, 9♥ from community = Three of a Kind (Aces)
    table.get_user_table_data_mut(player1_uid).unwrap().cards = vec![
        Card::new(Value::Ace, Suit::Spade),
        Card::new(Value::Ace, Suit::Heart),
        Card::new(Value::Two, Suit::Spade),
        Card::new(Value::Three, Suit::Spade),
    ];

    // Player 2: [K♥, K♣, Q♠, J♠]
    // Best hand: Use K♥, K♣ from hole + K♦, K♠, 9♥ from community = Four of a Kind (Kings)
    table.get_user_table_data_mut(player2_uid).unwrap().cards = vec![
        Card::new(Value::King, Suit::Heart),
        Card::new(Value::King, Suit::Club),
        Card::new(Value::Queen, Suit::Spade),
        Card::new(Value::Jack, Suit::Spade),
    ];

    // Perform showdown
    table.showdown().unwrap();

    // Player 2 should win with Four of a Kind
    let player2_balance_after = table.users.get(&player2_uid).unwrap().balance;
    assert!(player2_balance_after.0 > convert_to_e8s(100.0)); // Won the pot
}

#[test]
fn test_plo4_pot_limit_betting() {
    // Test that PLO4 enforces pot limit betting
    let mut table = Table::new(
        TableId(Principal::anonymous()),
        get_table_config(GameType::PotLimitOmaha4(convert_to_e8s(1.0)), 2),
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    );
    let user1 = create_user(
        Principal::from_text("2chl6-4hpzw-vqaaa-aaaaa-c").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );
    let user2 = create_user(
        Principal::from_text("br5f7-7uaaa-aaaaa-qaaca-cai").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );

    assert!(table.add_user(user1.clone(), 0, false).is_ok());
    assert!(table.add_user(user2.clone(), 1, false).is_ok());

    assert_eq!(
        table.start_betting_round(vec![0, 1, 2, 3, 4, 5, 6, 7, 8]),
        Ok((Vec::new(), Vec::new()))
    );

    // User1 tries to raise more than pot limit (pot is 3.0, so max raise is 3.0)
    assert!(table
        .bet(user1.principal_id, BetType::Raised(convert_to_e8s(10.0)))
        .is_err());

    // User1 can raise within pot limit
    assert!(table
        .bet(user1.principal_id, BetType::Raised(convert_to_e8s(3.0)))
        .is_ok());
}

#[test]
fn test_plo4_straight_flush_evaluation() {
    // Test PLO4 correctly finds straight flush using exactly 2 hole + 3 community
    let mut table = Table::new(
        TableId(Principal::anonymous()),
        get_table_config(GameType::PotLimitOmaha4(convert_to_e8s(1.0)), 2),
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    );
    let user1 = create_user(
        Principal::from_text("2chl6-4hpzw-vqaaa-aaaaa-c").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );
    let user2 = create_user(
        Principal::from_text("br5f7-7uaaa-aaaaa-qaaca-cai").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );

    assert!(table.add_user(user1.clone(), 0, false).is_ok());
    assert!(table.add_user(user2.clone(), 1, false).is_ok());

    let player1_uid = table.get_player_at_seat(0).unwrap();
    let player2_uid = table.get_player_at_seat(1).unwrap();

    assert_eq!(table.start_betting_round(vec![0, 1]), Ok((vec![], vec![])));
    assert_eq!(table.bet(player1_uid, BetType::Called), Ok(()));
    assert_eq!(table.bet(player2_uid, BetType::Called), Ok(()));

    // Community: [9♠, 10♠, J♠, 5♦, 2♣]
    table.community_cards = vec![
        Card::new(Value::Nine, Suit::Spade),
        Card::new(Value::Ten, Suit::Spade),
        Card::new(Value::Jack, Suit::Spade),
        Card::new(Value::Five, Suit::Diamond),
        Card::new(Value::Two, Suit::Club),
    ];

    // Player 1: [Q♠, K♠, A♥, 3♣]
    // Best: Q♠, K♠ + 9♠, 10♠, J♠ = Straight Flush (9-K)
    table.get_user_table_data_mut(player1_uid).unwrap().cards = vec![
        Card::new(Value::Queen, Suit::Spade),
        Card::new(Value::King, Suit::Spade),
        Card::new(Value::Ace, Suit::Heart),
        Card::new(Value::Three, Suit::Club),
    ];

    // Player 2: [8♠, 7♠, A♠, 2♠]
    // Best: 8♠, 7♠ + 9♠, 10♠, J♠ = Straight Flush (7-J) - lower than Player 1
    table.get_user_table_data_mut(player2_uid).unwrap().cards = vec![
        Card::new(Value::Eight, Suit::Spade),
        Card::new(Value::Seven, Suit::Spade),
        Card::new(Value::Ace, Suit::Spade),
        Card::new(Value::Two, Suit::Spade),
    ];

    table.showdown().unwrap();

    // Player 1 should win with higher straight flush
    let player1_balance = table.users.get(&player1_uid).unwrap().balance;
    assert!(player1_balance.0 > convert_to_e8s(100.0));
}

// ========== PLO5 Tests ==========

#[test]
fn test_plo5_deals_five_hole_cards() {
    // Test that PLO5 deals exactly 5 hole cards to each player
    let mut table = Table::new(
        TableId(Principal::anonymous()),
        get_table_config(GameType::PotLimitOmaha5(convert_to_e8s(1.0)), 2),
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    );
    let user1 = create_user(
        Principal::from_text("2chl6-4hpzw-vqaaa-aaaaa-c").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );
    let user2 = create_user(
        Principal::from_text("br5f7-7uaaa-aaaaa-qaaca-cai").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );

    assert!(table.add_user(user1.clone(), 0, false).is_ok());
    assert!(table.add_user(user2.clone(), 1, false).is_ok());

    assert_eq!(
        table.start_betting_round(vec![0, 1, 2, 3, 4, 5, 6, 7, 8]),
        Ok((Vec::new(), Vec::new()))
    );

    // Check that each player has exactly 5 hole cards
    assert_eq!(
        table
            .user_table_data
            .get(&user1.principal_id)
            .unwrap()
            .cards
            .len(),
        5
    );
    assert_eq!(
        table
            .user_table_data
            .get(&user2.principal_id)
            .unwrap()
            .cards
            .len(),
        5
    );
}

#[test]
fn test_plo5_hand_evaluation_exactly_two_hole_three_community() {
    // Test that PLO5 correctly evaluates hands using exactly 2 hole + 3 community
    let mut table = Table::new(
        TableId(Principal::anonymous()),
        get_table_config(GameType::PotLimitOmaha5(convert_to_e8s(1.0)), 2),
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    );
    let user1 = create_user(
        Principal::from_text("2chl6-4hpzw-vqaaa-aaaaa-c").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );
    let user2 = create_user(
        Principal::from_text("br5f7-7uaaa-aaaaa-qaaca-cai").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );

    assert!(table.add_user(user1.clone(), 0, false).is_ok());
    assert!(table.add_user(user2.clone(), 1, false).is_ok());

    let player1_uid = table.get_player_at_seat(0).unwrap();
    let player2_uid = table.get_player_at_seat(1).unwrap();

    assert_eq!(table.start_betting_round(vec![0, 1]), Ok((vec![], vec![])));
    assert_eq!(table.bet(player1_uid, BetType::Called), Ok(()));
    assert_eq!(table.bet(player2_uid, BetType::Called), Ok(()));

    // Community: [A♦, A♠, K♥, Q♣, J♦]
    table.community_cards = vec![
        Card::new(Value::Ace, Suit::Diamond),
        Card::new(Value::Ace, Suit::Spade),
        Card::new(Value::King, Suit::Heart),
        Card::new(Value::Queen, Suit::Club),
        Card::new(Value::Jack, Suit::Diamond),
    ];

    // Player 1: [A♥, A♣, 2♠, 3♠, 4♠]
    // Best: A♥, A♣ + A♦, A♠, K♥ = Four of a Kind (Aces)
    table.get_user_table_data_mut(player1_uid).unwrap().cards = vec![
        Card::new(Value::Ace, Suit::Heart),
        Card::new(Value::Ace, Suit::Club),
        Card::new(Value::Two, Suit::Spade),
        Card::new(Value::Three, Suit::Spade),
        Card::new(Value::Four, Suit::Spade),
    ];

    // Player 2: [K♠, K♣, Q♠, J♠, 10♠]
    // Best: K♠, Q♠ + A♦, A♠, K♥ = Full House (Aces over Kings)
    table.get_user_table_data_mut(player2_uid).unwrap().cards = vec![
        Card::new(Value::King, Suit::Spade),
        Card::new(Value::King, Suit::Club),
        Card::new(Value::Queen, Suit::Spade),
        Card::new(Value::Jack, Suit::Spade),
        Card::new(Value::Ten, Suit::Spade),
    ];

    table.showdown().unwrap();

    // Player 1 should win with Four of a Kind
    let player1_balance = table.users.get(&player1_uid).unwrap().balance;
    assert!(player1_balance.0 > convert_to_e8s(100.0));
}

#[test]
fn test_plo5_pot_limit_betting() {
    // Test that PLO5 enforces pot limit betting
    let mut table = Table::new(
        TableId(Principal::anonymous()),
        get_table_config(GameType::PotLimitOmaha5(convert_to_e8s(1.0)), 2),
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    );
    let user1 = create_user(
        Principal::from_text("2chl6-4hpzw-vqaaa-aaaaa-c").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );
    let user2 = create_user(
        Principal::from_text("br5f7-7uaaa-aaaaa-qaaca-cai").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );

    assert!(table.add_user(user1.clone(), 0, false).is_ok());
    assert!(table.add_user(user2.clone(), 1, false).is_ok());

    assert_eq!(
        table.start_betting_round(vec![0, 1, 2, 3, 4, 5, 6, 7, 8]),
        Ok((Vec::new(), Vec::new()))
    );

    // User1 tries to raise more than pot limit
    assert!(table
        .bet(user1.principal_id, BetType::Raised(convert_to_e8s(10.0)))
        .is_err());

    // User1 can raise within pot limit
    assert!(table
        .bet(user1.principal_id, BetType::Raised(convert_to_e8s(3.0)))
        .is_ok());
}

#[test]
fn test_plo5_full_house_evaluation() {
    // Test PLO5 correctly finds full house using exactly 2 hole + 3 community
    let mut table = Table::new(
        TableId(Principal::anonymous()),
        get_table_config(GameType::PotLimitOmaha5(convert_to_e8s(1.0)), 2),
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    );
    let user1 = create_user(
        Principal::from_text("2chl6-4hpzw-vqaaa-aaaaa-c").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );
    let user2 = create_user(
        Principal::from_text("br5f7-7uaaa-aaaaa-qaaca-cai").expect("Could not decode principal"),
        convert_to_e8s(100.0),
    );

    assert!(table.add_user(user1.clone(), 0, false).is_ok());
    assert!(table.add_user(user2.clone(), 1, false).is_ok());

    let player1_uid = table.get_player_at_seat(0).unwrap();
    let player2_uid = table.get_player_at_seat(1).unwrap();

    assert_eq!(table.start_betting_round(vec![0, 1]), Ok((vec![], vec![])));
    assert_eq!(table.bet(player1_uid, BetType::Called), Ok(()));
    assert_eq!(table.bet(player2_uid, BetType::Called), Ok(()));

    // Community: [K♦, K♠, 9♥, 5♣, 3♦]
    table.community_cards = vec![
        Card::new(Value::King, Suit::Diamond),
        Card::new(Value::King, Suit::Spade),
        Card::new(Value::Nine, Suit::Heart),
        Card::new(Value::Five, Suit::Club),
        Card::new(Value::Three, Suit::Diamond),
    ];

    // Player 1: [K♥, 9♠, 2♠, 3♠, 4♠]
    // Best: K♥, 9♠ + K♦, K♠, 9♥ = Full House (Kings over Nines)
    // Uses 1 King + 1 Nine from hole, 2 Kings + 1 Nine from community = 3 Kings + 2 Nines
    table.get_user_table_data_mut(player1_uid).unwrap().cards = vec![
        Card::new(Value::King, Suit::Heart),
        Card::new(Value::Nine, Suit::Spade),
        Card::new(Value::Two, Suit::Spade),
        Card::new(Value::Three, Suit::Spade),
        Card::new(Value::Four, Suit::Spade),
    ];

    // Player 2: [K♣, 9♣, 2♣, 3♣, 4♣]
    // Best: K♣, 9♣ + K♦, K♠, 9♥ = Full House (Kings over Nines) - same as Player 1
    // Uses 1 King + 1 Nine from hole, 2 Kings + 1 Nine from community = 3 Kings + 2 Nines
    // Both players have identical Full House (Kings over Nines), so pot should split
    table.get_user_table_data_mut(player2_uid).unwrap().cards = vec![
        Card::new(Value::King, Suit::Club),
        Card::new(Value::Nine, Suit::Club),
        Card::new(Value::Two, Suit::Club),
        Card::new(Value::Three, Suit::Club),
        Card::new(Value::Four, Suit::Club),
    ];

    table.showdown().unwrap();

    // Both have same full house, should split pot
    let player1_balance = table.users.get(&player1_uid).unwrap().balance;
    let player2_balance = table.users.get(&player2_uid).unwrap().balance;
    // Both should have won something (pot split)
    assert!(player1_balance.0 >= convert_to_e8s(100.0));
    assert!(player2_balance.0 >= convert_to_e8s(100.0));
}