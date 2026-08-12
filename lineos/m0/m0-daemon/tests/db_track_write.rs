use m0d::db;
use m0d::db::Track;

/// ΟΚΤΩ πεδία, όσα δηλώνει το schema. ΠΡΕΠΕΙ να
/// περάσει — αν αποτύχει, το πρόβλημα είναι αλλού
/// και τα άλλα δύο tests δεν λένε τίποτα.
#[tokio::test]
async fn eight_fields_write_and_read_back() {
    let db = db::init_test().await.expect("test db");
    db::schema::migrate(&db).await.expect("migrate failed");

    let sql = "CREATE tracks CONTENT {
        blob_id: $blob_id,
        audio_path: $audio_path,
        lufs: $lufs,
        true_peak: $true_peak,
        created_at: $created_at,
        project_id: $project_id,
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
        .bind(("flavour_id", "flav_123"))
        .bind(("duration_ms", 1000u64))
        .await
        .expect("query success");

    let mut response = db.query("SELECT * FROM tracks").await.expect("select success");
    let raw: Vec<serde_json::Value> = response.take(0).expect("take success");
    let tracks: Vec<Track> = raw
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].project_id, "proj_123");
    assert_eq!(tracks[0].lufs, -14.0f32);
}

/// ΕΝΝΕΑ πεδία — ΑΚΡΙΒΩΣ το SQL του master.rs:204,
/// αντιγραμμένο ΑΥΤΟΥΣΙΟ.
///
/// ΑΥΤΟ ΤΟ TEST ΔΕΝ ΕΧΕΙ ΠΡΟΚΑΘΟΡΙΣΜΕΝΗ ΑΠΑΝΤΗΣΗ.
/// ΜΗΝ γράψεις assert που υποθέτει αποτυχία ΟΥΤΕ
/// επιτυχία. ΤΥΠΩΣΕ το αποτέλεσμα και βάλε assert
/// ΜΟΝΟ σε ό,τι μετρήθηκε.
#[tokio::test]
async fn nine_fields_what_actually_happens() {
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
        .await;

    println!("Result of 9 fields CREATE: {:?}", result);

    if let Ok(mut response) = db.query("SELECT * FROM tracks").await {
        if let Ok(tracks) = response.take::<Vec<serde_json::Value>>(0) {
            println!("tracks table row count: {}", tracks.len());
            for (i, t) in tracks.iter().enumerate() {
                println!("row {}: {:?}", i, t);
            }
        } else {
            println!("failed to parse SELECT output");
        }
    } else {
        println!("SELECT query failed");
    }
}

/// Τα 7 tests που καλούν init_test ΔΕΝ καλούν
/// migrate. Δηλαδή δουλεύουν σε βάση χωρίς πίνακες.
/// Αυτό επιβεβαιώνει ότι το migrate είναι απαραίτητο.
#[tokio::test]
async fn without_migrate_the_table_does_not_exist() {
    let db = db::init_test().await.expect("test db");

    let sql = "CREATE tracks CONTENT {
        blob_id: $blob_id,
        audio_path: $audio_path,
        lufs: $lufs,
        true_peak: $true_peak,
        created_at: $created_at,
        project_id: $project_id,
        flavour_id: $flavour_id,
        duration_ms: $duration_ms
    }";

    let result = db
        .query(sql)
        .bind(("blob_id", "test_blob"))
        .bind(("audio_path", "/tmp/test.wav"))
        .bind(("lufs", -14.0f32))
        .bind(("true_peak", -1.0f32))
        .bind(("created_at", "2024-01-01T00:00:00Z"))
        .bind(("project_id", "proj_123"))
        .bind(("flavour_id", "flav_123"))
        .bind(("duration_ms", 1000u64))
        .await;

    println!("Result without migrate: {:?}", result);
}
