use std::time::Duration;
use thiserror::Error;
use tracing::{info, instrument, warn};
use tracing_config_helper::{Env, TracerConfig};

#[derive(Debug, Error)]
enum MyErr {}

#[derive(Debug)]
struct MyKey {
    my_key_val: i32,
}

fn text_570_chars() -> String {
    "LoremIpsumissimplydummytextoftheprintingandtypesettingindustry.LoremIpsumhasbeentheindustry'sstandarddummytexteversincethe1500s,whenanunknownprintertookagalleyoftypeandscrambledittomakeatypespecimenbook.Ithassurvivednotonlyfivecenturies,butalsotheleapintoelectronictypesetting,remainingessentiallyunchanged.Itwaspopularisedinthe1960swiththereleaseofLetrasetsheetscontainingLoremIpsumpassages,andmorerecentlywithdesktoppublishingsoftwarelikeAldusPageMakerincludingversionsofLoremIpsum.".to_string()
}

fn biggest_text() -> String {
    let mut max_text_val = vec![];

    let max_chars = 1_500_000;
    for i in 0..max_chars - 2 {
        if i % 1000 == 0 {
            max_text_val.push('\n');
        } else {
            max_text_val.push('a');
        }
    }
    max_text_val.push('b');
    max_text_val.push('c');
    max_text_val.push('T');
    max_text_val.push('R');
    max_text_val.push('U');
    let max_text_val: String = max_text_val.into_iter().collect();
    max_text_val
}

fn simple_orphan_logs() {
    let key = MyKey { my_key_val: 1 };
    info!(?key, "logging struct in key value");
    warn!(
        firstKv = "Val1",
        secondKey = "Val2",
        thirdKey = "Val3",
        "multiple key val"
    );
    info!(LoremIpsumissimplydummytextoftheprintingandtypesettingindusawdadawdadatryLoremIpsumhasbeentheindustrysstandarddummytexteversincethe1500swhenanunknownprintertookagalleyoftypeandscrambledittomakeatypespecimenbookIthassurvivednr31iesShouldTruncateJustAboutNowTruncatedPart=23, "truncated key with 23 as val");
    let long_text_570_chars = text_570_chars();
    info!(
        long_text_570_chars = long_text_570_chars,
        "logging struct in key value"
    );
    let max_text_val = biggest_text();
    info!(%max_text_val, "This is the max val we allow");

    // warn!(keyw = "somee");
}

#[instrument(fields(LoremIpsumissimplydummytextoftheprintingandtypesettingindusawdadawdadatryLoremIpsumhasbeentheindustrysstandarddummytexteversincethe1500swhenanunknownprintertookagalleyoftypeandscrambledittomakeatypespecimenbookIthassurvivednr31iesShouldTruncateJustAboutNowTruncatedPart=32))]
fn span_tests() {
    let max_text_val = biggest_text();
    multiple_fields();
    warn!(
        firstKv = "Val1",
        secondKey = "Val2",
        thirdKey = "Val3",
        "multiple key val"
    );
    function_taking_argument(
        max_text_val.clone(),
        SampleStructArg {
            some_field: "sample 1".to_string(),
            some_other_field: "sample 2".to_string(),
        },
    );

    // info!(LoremIpsumissimplydummytextoftheprintingandtypesettingindusawdadawdadatryLoremIpsumhasbeentheindustrysstandarddummytexteversincethe1500swhenanunknownprintertookagalleyoftypeandscrambledittomakeatypespecimenbookIthassurvivednr31iesShouldTruncateJustAboutNowTruncatedPart=23, "truncated key with 23 as val");
    // info!(%max_text_val, "This is the max val we allow");
}

#[derive(Debug, Clone)]
struct SampleStructArg {
    some_field: String,
    some_other_field: String,
}
#[instrument]
fn span_small_tests() {
    multiple_fields();
    warn!(
        firstKv = "Val1",
        secondKey = "Val2",
        thirdKey = "Val3",
        "multiple key val"
    );
    let a = SampleStructArg {
        some_field: "sample 1".to_string(),
        some_other_field: "sample 2".to_string(),
    };
    function_taking_argument("sample arg".to_string(), a.clone());
    function_taking_argument(
        r#"Multi
line
sample
arg"#
            .to_string(),
        SampleStructArg {
            some_field: "sample 1".to_string(),
            some_other_field: "sample 2".to_string(),
        },
    );

    // info!(LoremIpsumissimplydummytextoftheprintingandtypesettingindusawdadawdadatryLoremIpsumhasbeentheindustrysstandarddummytexteversincethe1500swhenanunknownprintertookagalleyoftypeandscrambledittomakeatypespecimenbookIthassurvivednr31iesShouldTruncateJustAboutNowTruncatedPart=23, "truncated key with 23 as val");
    // info!(%max_text_val, "This is the max val we allow");
}

#[instrument(fields(%arg))]
fn function_taking_argument(arg: String, struct_arg: SampleStructArg) {
    info!("inside function_taking_argument");
}
#[instrument(fields(one = "first", two = "second", three = 3))]
fn multiple_fields() {
    info!("inside multiple_fields");
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("Hello, world!");
    let tracer_config = TracerConfig::new(
        Env::Local,
        env!("CARGO_BIN_NAME").to_string(),
        "http://127.0.0.1:4200".to_string(),
    );
    let flush_requester = tracing_config_helper::setup_tracer_client_or_panic(tracer_config).await;
    // simple_orphan_logs();
    span_small_tests();
    // span_tests();
    // let mut my_vec = vec![1];
    // for i in 0..1_000_000 {
    //     my_vec.push(i);
    // }
    // info!(my_vec = ?my_vec);

    flush_requester
        .flush(Duration::from_secs(100))
        .await
        .unwrap();
}
