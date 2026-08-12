use m0d::db;
use m0d::db::Track;

/// ΟΛΑ τα δηλωμένα πεδία, όσα λέει το DEFINE TABLE.
/// ΠΡΕΠΕΙ να περάσει — αν αποτύχει, το struct, ο
/// ορισμός και το query ξαναδιαφώνησαν.
///
/// ΗΤΑΝ "eight_fields": το schema δήλωνε οκτώ και
/// το CREATE του master.rs έστελνε εννέα. Τώρα
/// δηλώνει εννέα.
#[tokio::test]
async fn all_declared_fields_round_trip() {
    let db = db::init_test().await.expect("test db");
    db::schema::migrate(&db).await.expect("migrate failed");

    let sql = "CREATE tracks CONTENT {
        blob_id: $blob_id,
        audio_path: $audio_path,
        lufs: $lufs,
        true_peak: $true_peak,
        created_at: $created_at,
        project_id: $project_id,
        track_id: $track_id,
        flavour_id: $flavour_id,
        duration_ms: $duration_ms
    }";

    let _ = db
        .query(sql)
        .bind(("blob_id", "test_blob"))
        .bind(("audio_path", "/tmp/test.wav"))
        .bind(("lufs", -14.0f32))
        .bind(("true_peak", -1.0f32))
        .bind(("created_at", "2024-01-01T00:00:00Z"))
        .bind(("project_id", "proj_123"))
        .bind(("track_id", "track_123"))
        .bind(("flavour_id", "flav_123"))
        .bind(("duration_ms", 1000u64))
        .await
        .expect("outer query error")
        .check()
        .expect("inner query error");

    let mut response = db.query("SELECT * FROM tracks").await.expect("select success");
    let raw: Vec<serde_json::Value> = response.take(0).expect("take success");
    let tracks: Vec<Track> = raw
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].project_id, "proj_123");
    assert_eq!(tracks[0].track_id, "track_123");
    assert_eq!(tracks[0].lufs, -14.0f32);
}

/// Το SCHEMAFULL απορρίπτει άγνωστο πεδίο και
/// ΟΛΟΚΛΗΡΗ την εγγραφή μαζί.
///
/// ΜΕΤΡΗΜΕΝΟ, ΟΧΙ ΥΠΟΤΕΘΕΝ: αυτό ακριβώς συνέβαινε
/// με το track_id — "Found field 'track_id', but no
/// such field exists for table 'tracks'", row count 0,
/// σε κάθε master, χωρίς log.
///
/// Ο μάρτυρας μένει ώστε η επόμενη διαφωνία σχήματος
/// να πιαστεί εδώ αντί για την παραγωγή.
#[tokio::test]
async fn unknown_field_is_rejected_by_schemafull() {
    let db = db::init_test().await.expect("test db");
    db::schema::migrate(&db).await.expect("migrate failed");

    let sql = "CREATE tracks CONTENT {
        blob_id: $blob_id,
        audio_path: $audio_path,
        lufs: $lufs,
        true_peak: $true_peak,
        created_at: $created_at,
        project_id: $project_id,
        track_id: $track_id,
        flavour_id: $flavour_id,
        duration_ms: $duration_ms,
        definitely_not_a_field: $fake
    }";

    let result = db
        .query(sql)
        .bind(("blob_id", "test_blob"))
        .bind(("audio_path", "/tmp/test.wav"))
        .bind(("lufs", -14.0f32))
        .bind(("true_peak", -1.0f32))
        .bind(("created_at", "2024-01-01T00:00:00Z"))
        .bind(("project_id", "proj_123"))
        .bind(("track_id", "track_123"))
        .bind(("flavour_id", "flav_123"))
        .bind(("duration_ms", 1000u64))
        .bind(("fake", "some_value"))
        .await;

    assert!(result.is_ok(), "Outer result is Ok, query parses");
    let checked_result = result.unwrap().check();
    assert!(checked_result.is_err(), "Inner result is Err via .check()");

    println!("Result of invalid CREATE with check(): {:?}", checked_result);

    let mut response = db.query("SELECT * FROM tracks").await.expect("select ok");
    let tracks: Vec<serde_json::Value> = response.take(0).expect("take ok");
    
    println!("tracks table row count: {}", tracks.len());
    assert_eq!(tracks.len(), 0, "No tracks should be written when unknown field fails validation");
}

/// ΧΩΡΙΣ migrate η SurrealDB φτιάχνει τον πίνακα
/// σιωπηλά ως SCHEMALESS και δέχεται τα πάντα.
/// Επτά tests καλούν init_test χωρίς migrate —
/// δουλεύουν σε βάση που δεν μοιάζει με την παραγωγή.
#[tokio::test]
async fn without_migrate_the_table_is_schemaless() {
    let db = db::init_test().await.expect("test db");

    let sql = "CREATE tracks CONTENT {
        blob_id: $blob_id,
        audio_path: $audio_path,
        lufs: $lufs,
        true_peak: $true_peak,
        created_at: $created_at,
        project_id: $project_id,
        track_id: $track_id,
        flavour_id: $flavour_id,
        duration_ms: $duration_ms,
        definitely_not_a_field: $fake
    }";

    let result = db
        .query(sql)
        .bind(("blob_id", "test_blob"))
        .bind(("audio_path", "/tmp/test.wav"))
        .bind(("lufs", -14.0f32))
        .bind(("true_peak", -1.0f32))
        .bind(("created_at", "2024-01-01T00:00:00Z"))
        .bind(("project_id", "proj_123"))
        .bind(("track_id", "track_123"))
        .bind(("flavour_id", "flav_123"))
        .bind(("duration_ms", 1000u64))
        .bind(("fake", "some_value"))
        .await
        .expect("outer query error");

    let checked_result = result.check();
    assert!(checked_result.is_ok(), "Without migrate, schemaless table accepts any field");

    println!("Result without migrate: {:?}", checked_result);
}
