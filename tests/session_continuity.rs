use bastion::session::SessionManager;
use bastion::types::{Message, Role, MessageContent};

#[tokio::test]
async fn messages_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("test.db").to_str().unwrap().to_owned();

    let sm = SessionManager::new(&db);
    sm.init_schema().await.unwrap();
    let sid = sm.create_session().await.unwrap();

    sm.append(&sid, Message { role: Role::User, content: MessageContent::Text("hello".into()) }, None).await.unwrap();
    sm.append(&sid, Message { role: Role::Assistant, content: MessageContent::Text("hi".into()) }, Some(42)).await.unwrap();

    // Simulate restart
    let sm2 = SessionManager::new(&db);
    let msgs = sm2.load_recent(&sid).await.unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, Role::User);
    assert_eq!(msgs[1].role, Role::Assistant);
}

#[tokio::test]
async fn orphaned_tool_result_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("test.db").to_str().unwrap().to_owned();
    let sm = SessionManager::new(&db);
    sm.init_schema().await.unwrap();
    let sid = sm.create_session().await.unwrap();
    sm.append(&sid, Message { role: Role::User, content: MessageContent::Text("q".into()) }, None).await.unwrap();

    // Try to append Tool without preceding Assistant — must fail
    let result = sm.append(&sid, Message { role: Role::Tool, content: MessageContent::Text("result".into()) }, None).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Orphaned") || err.contains("orphaned") || err.contains("tool_use"), "got: {}", err);
}

#[tokio::test]
async fn load_most_recent_id_works() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("test.db").to_str().unwrap().to_owned();
    let sm = SessionManager::new(&db);
    sm.init_schema().await.unwrap();
    assert!(sm.load_most_recent_id().await.unwrap().is_none());
    let sid = sm.create_session().await.unwrap();
    assert_eq!(sm.load_most_recent_id().await.unwrap(), Some(sid));
}

#[tokio::test]
async fn budget_check_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("test.db").to_str().unwrap().to_owned();
    let sm = SessionManager::new(&db);
    sm.init_schema().await.unwrap();
    assert!(sm.check_budget(5.0).await.unwrap()); // no spend yet → under budget
    sm.update_budget(4.99).await.unwrap();
    assert!(sm.check_budget(5.0).await.unwrap()); // still under
    sm.update_budget(0.02).await.unwrap();
    assert!(!sm.check_budget(5.0).await.unwrap()); // over budget
}
