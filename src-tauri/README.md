# EduWork Tools · Rust 백엔드

최신 기능·실행 환경은 [루트 README](../README.md), 제한·백업/복원·출시 게이트는 [Phase 8 점검 문서](../docs/RELEASE_READINESS.md)를 참고하세요.

`src/workflows.rs`는 workflow_load/save/run 명령과 JSON 기록을 담당합니다. 이미지/PDF/Excel의 기존 실행 루틴은 유지하고 `src/output.rs`에서 완성된 임시 파일을 동기화한 뒤 persist_noclobber로 게시합니다. 다중 파일 전체의 원자성은 제공하지 않으며 완료된 파일만 실행 기록에 추가합니다. JSON은 같은 디렉터리 임시 파일을 원자적으로 교체합니다. 단일 프로세스 Mutex로 저장과 실행을 직렬화하며 여러 프로세스의 기록 병합은 지원하지 않습니다.

테스트는 `cargo test --lib`, 정적 검증은 `cargo check`와 `cargo fmt --check`입니다. 워크플로 테스트는 실제 이미지/PDF/XLSX 출력과 재로드, 실패 영속화, 템플릿 및 기록 보존을 검사합니다. Windows 링크 테스트는 OS가 링크 생성을 허용하지 않으면 사유를 출력하고 건너뜁니다. 취소·백분율 진행률·강제 종료 후 자동 복구는 구현하지 않았습니다.
