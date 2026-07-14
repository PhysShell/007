fn op_02() {
    let e = Error::new();
    log::warn!("Error while handling op 02");
    return Err(Error::from(e));
}
