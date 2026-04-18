#[test]
fn test_dummy() {
    assert_eq!(2 + 2, 4);
}
// B21: Monday gate fixed-date tests
// See chat.md B21 and DeFa_PSP_Architecture_v2.md section 2, 7, and Monday gate rules
// This test uses known Unix timestamps to verify the on-chain Monday gate logic

use defa_psp_pool::state::day_of_week;

#[test]
fn test_monday_gate_known_dates() {
    // 1704067200 = Monday 2024-01-01 00:00:00 UTC
    let monday = 1704067200;
    // 1704067200 + 86400 = Tuesday
    let tuesday = monday + 86400;
    // 1704067200 + (6 * 86400) = Sunday
    let sunday = monday + 6 * 86400;
    // 1704067200 + (7 * 86400) = next Monday
    let next_monday = monday + 7 * 86400;

    // Formula: ((timestamp / 86400) + 4) % 7, Monday=1
    assert_eq!(day_of_week(monday), 1, "Monday should be 1");
    assert_eq!(day_of_week(tuesday), 2, "Tuesday should be 2");
    assert_eq!(day_of_week(sunday), 0, "Sunday should be 0");
    assert_eq!(day_of_week(next_monday), 1, "Next Monday should be 1");
}
