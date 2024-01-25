use std::time::Duration;
use thiserror::Error;
use tokio::join;
use tracing::{debug, error, info, instrument, trace, warn};
use tracing_config_helper::{Env, ServiceId, TracerConfig};

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

#[instrument(skip_all)]
fn only_warning() {
    warn!("Simple Warning");
}
#[instrument(skip_all)]
fn only_error() {
    error!("Simple Error");
}
#[instrument(skip_all)]
async fn basic_tracer_trace_tests() {
    trace!(r#"Sample Trace event"#);
    debug!("Sample Debug event");
    info!("Sample Info event");
    warn!("Sample Warn event");
    error!(
        r#"Sample Error event 
Second Line!"#
    );
    let basic_struct = BasicStructArg::new();
    nested_function_taking_1s(
        &basic_struct,
        12,
        "Some Extra String\nSecond Line!".to_string(),
    )
    .await;
    let _ = join!(
        nested_function_taking_1s(&basic_struct, 12, "Two concurrent calls 1".to_string()),
        nested_function_taking_1s(&basic_struct, 12, "Two concurrent calls 2".to_string())
    );
}

#[derive(Debug, Clone)]
struct BasicStructArg {
    int_field: i32,
    float_field: f32,
    string_field: String,
}
impl BasicStructArg {
    pub fn new() -> Self {
        BasicStructArg {
            int_field: 2,
            float_field: 2.5,
            string_field: "This is a Sample String Field".to_string(),
        }
    }
}
#[instrument(fields(extra_string_input_as_display=%extra_string_input))]
async fn nested_function_taking_1s(
    struct_as_input: &BasicStructArg,
    extra_number_input: i32,
    extra_string_input: String,
) {
    tokio::time::sleep(Duration::from_secs(1)).await;
    info!(?struct_as_input, "input debug struct as key-val");
    info!(extra_number_input, "extra_number_input as key-val");
    info!(extra_string_input, "extra_string_input as key-val");
}

fn simple_orphan_logs_test() {
    trace!("Sample Trace orphan event");
    debug!("Sample Debug orphan event");
    info!("Sample Info orphan event");
    warn!("Sample Warn orphan event");
    error!("Sample Error orphan event");
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("Hello, world from Tracer Test");
    let tracer_config = TracerConfig::new(
        ServiceId {
            name: env!("CARGO_BIN_NAME").to_string(),
            env: Env::Local,
        },
        "http://127.0.0.1:4200".to_string(),
    );
    let flush_requester = tracing_config_helper::setup_tracer_client_or_panic(tracer_config).await;
    only_warning();
    only_error();
    simple_orphan_logs_test();
    basic_tracer_trace_tests().await;

    flush_requester
        .flush(Duration::from_secs(100))
        .await
        .unwrap();
}
