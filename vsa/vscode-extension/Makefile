.PHONY: install compile watch test lint package clean

# Install dependencies
install:
	npm install

# Compile TypeScript
compile:
	npm run compile

# Watch mode for development
watch:
	npm run watch

# Run tests
test:
	npm run test

# Lint code
lint:
	npm run lint

# Package extension
package: compile
	npx vsce package

# Clean build artifacts
clean:
	rm -rf out/
	rm -f *.vsix

# Development workflow
dev: install compile
	@echo "✅ Extension ready for development"
	@echo "Press F5 in VS Code to test"

# QA checks
qa: lint compile
	@echo "✅ QA checks passed"

# Full build and package
build: clean install qa package
	@echo "✅ Extension packaged successfully"

