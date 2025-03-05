use crate::Client;

#[test]
fn should_be_clone() {
    let client = Client::new_with_box_error();
    let _ = client.clone();
}
