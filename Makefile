.PHONY: test-engine copy-test-data bundle-test-data download-test-data extract-test-data

test-engine:
	cargo test --test engine_test -- --nocapture