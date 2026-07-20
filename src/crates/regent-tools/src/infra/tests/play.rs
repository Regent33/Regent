//! Unit tests for `play` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

#[test]
fn picks_official_over_live_lyrics_cover() {
    let out = "ID1\tAC/DC - Back In Black (Official 4K Video)\tAC/DC\t1214313302\n\
               ID2\tAC/DC - Back In Black (Lyrics)\t7clouds Rock\t9142071\n\
               ID3\tAC/DC - Back In Black (Live At River Plate)\tAC/DC\t71434597\n\
               ID4\tBack In Black cover\tSome Band\t500000";
    assert_eq!(pick_best(out, "back in black acdc").unwrap().0, "ID1");
}

#[test]
fn respects_an_explicit_live_request() {
    // Intent beats popularity: the studio cut has 200x the views, but the
    // user asked for "live", so the live row wins.
    let out = "ID1\tSong (Official Video)\tArtistVEVO\t1000000000\n\
               ID2\tSong (Live at Wembley)\tArtist\t5000000";
    assert_eq!(pick_best(out, "song live").unwrap().0, "ID2");
}

#[test]
fn none_when_no_rows() {
    assert!(pick_best("", "anything").is_none());
}

#[test]
fn named_artist_beats_a_more_popular_different_cover() {
    let out = "WRONG\tAkon - Don't Matter (Cover by Katzi Reign)\tKatzi Reign\t9000000\n\
               RIGHT\tPlatinum Blues - Don't Matter (Akon Cover)\tPlatinum Blues\t15000";
    assert_eq!(
        pick_best(out, "Don't Matter cover by Platinum Blues")
            .unwrap()
            .0,
        "RIGHT"
    );
}
