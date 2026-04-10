.PHONY: test-engine copy-test-data bundle-test-data download-test-data extract-test-data

test-engine:
	cargo test --test engine_test -- --nocapture

copy-test-data:
	mkdir -p tests/data
	cp -r triangle.js/test/data/replays tests/data/replays
	cp triangle.js/test/data/replays.tar.gz tests/data/replays.tar.gz

bundle-test-data:
	tar -C tests/data/replays -czf tests/data/replays.tar.gz .

download-test-data:
	git lfs fetch --include="tests/data/**" --exclude=""
	git lfs checkout tests/data
	$(MAKE) extract-test-data

extract-test-data:
	mkdir -p tests/data/replays
	tar -xzf tests/data/replays.tar.gz -C tests/data/replays
	@if [ -d tests/data/replays/replays ]; then mv tests/data/replays/replays/* tests/data/replays/ && rmdir tests/data/replays/replays; fi