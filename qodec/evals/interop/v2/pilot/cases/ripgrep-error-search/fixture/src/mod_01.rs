fn op_01() {
    let e = Error::new();
    log::warn!("Error while handling op 01");
    return Err(Error::from(e));
}
