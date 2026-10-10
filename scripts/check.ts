import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const files = execFileSync('git', ['ls-files', '-z'], { cwd: root, encoding: 'utf8', timeout: 5000, windowsHide: true })
  .split('\0').filter(Boolean);
if (!files.length) throw new Error('No tracked files to check.');
const decoder = new TextDecoder('utf-8', { fatal: true });
for (const file of files) {
  const bytes = readFileSync(new URL('../' + file, import.meta.url));
  if (bytes.includes(0)) throw new Error(`Unexpected binary publication file: ${file}`);
  const text = decoder.decode(bytes);
  if (file.endsWith('.ts')) {
    if (text.split('\n').length > 800) throw new Error(`Source file exceeds 800 lines: ${file}`);
    execFileSync(process.execPath, ['--check', file], { cwd: root, timeout: 5000, windowsHide: true, stdio: 'pipe' });
  }
  if (file.endsWith('.ps1')) {
    const path = file.replaceAll("'", "''");
    const script = `$tokens=$null;$errors=$null;[System.Management.Automation.Language.Parser]::ParseFile('${path}',[ref]$tokens,[ref]$errors)|Out-Null;if($errors.Count){$errors|ForEach-Object{$_.Message};exit 1}`;
    execFileSync('pwsh', ['-NoProfile', '-Command', script], { cwd: root, timeout: 5000, windowsHide: true, stdio: 'pipe' });
  }
}
console.log(`Validated UTF-8 and native source syntax: ${files.length} tracked files.`);
