use shadowplay::ShadowPlayActor;

fn main() -> Result<(), shadowplay::Error> {
    let shadowplay = ShadowPlayActor::build()?;

    println!("{:#?}", shadowplay.microphone_get_all());

    // shadowplay.microphone_change("{0.0.0.00000000}.{aa-bb-cc-123-456}")?;

    Ok(())
}
