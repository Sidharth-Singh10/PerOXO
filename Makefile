DIRS := chat-service peroxo user-service

.PHONY: all fmt

# Default target
all: fmt

# Run cargo fmt in each directory
fmt:
	@for dir in $(DIRS); do \
		echo "Formatting in $$dir..."; \
		(cd $$dir && cargo fmt); \
	done
