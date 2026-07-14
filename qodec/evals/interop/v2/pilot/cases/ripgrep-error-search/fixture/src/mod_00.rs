fn op_00() {
    let e = Error::new();
    log::warn!("Error while handling op 00");
    return Err(Error::from(e));
}
