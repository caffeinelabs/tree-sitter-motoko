// Formats the given files in place with the TypeScript prettier-plugin-motoko, the reference mo-fmt is compared against.
// Usage: node ts-format.mjs <plugin checkout> <preserve|moc2> <files…>
import { readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const [pluginDir, motokoSyntax, ...files] = process.argv.slice(2);
const require = createRequire(join(pluginDir, 'package.json'));
const prettierModule = await import(pathToFileURL(require.resolve('prettier')).href);
const prettier = prettierModule.default ?? prettierModule;
const plugin = (await import(pathToFileURL(join(pluginDir, 'lib/index.js')).href)).default;

let failed = 0;
for (const file of files) {
    const source = readFileSync(file, 'utf8');
    try {
        const out = await prettier.format(source, { parser: 'motoko', plugins: [plugin], motokoSyntax, filepath: file });
        if (out !== source) writeFileSync(file, out);
    } catch (e) {
        failed += 1;
        if (!(e instanceof SyntaxError)) console.error(`${file}: ${e.message.split('\n')[0]}`);
    }
}
console.error(`ts: ${files.length} files, ${failed} left unformatted`);
