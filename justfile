default:
    watchexec -w grammar.js -w test/ just test

play:
    npx tree-sitter generate && npx tree-sitter build --wasm && npx tree-sitter playground

test:
    npx tree-sitter generate
    npx tree-sitter test

test-generate:
    rm -rf test/corpus/generated
    cd dev && cargo run
    npx tree-sitter generate
    npx tree-sitter test --update
    npx tree-sitter test

accept:
    npx tree-sitter generate
    npx tree-sitter test --update

packages:
    npx tree-sitter generate
    dev/parse-packages.sh
