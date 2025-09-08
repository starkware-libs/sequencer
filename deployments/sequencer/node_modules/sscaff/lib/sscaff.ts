import { promises as fs } from 'fs';
import * as path from 'path';
import { substitute } from './substitute';

const hooksFile = '.hooks.sscaff.js';

/**
 * Copy all files from `templateDir` to `targetDir` and substitute all variables
 * in file names and their contents. Substitutions take the form `{{ key }}`.
 *
 * @param sourceDir
 * @param targetDir
 * @param variables
 */
export async function sscaff(sourceDir: string, targetDir: string, variables: { [key: string]: string } = { }) {
  sourceDir = path.resolve(sourceDir);
  targetDir = path.resolve(targetDir);

  await fs.mkdir(targetDir, { recursive: true });

  const hooks = loadHooks();

  if (!variables.$base) {
    variables.$base = path.basename(targetDir);
  }

  const restore = process.cwd();
  try {
    process.chdir(targetDir);
    await executePreHook();
    await processDirectory('.');
    await executePostHook();
  } finally {
    process.chdir(restore);
  }

  async function processDirectory(subdir: string) {
    const subPath = path.join(sourceDir, subdir);
    for (const file of await fs.readdir(subPath)) {

      if (file === hooksFile) {
        continue;
      }

      const sourcePath = path.join(subPath, file);

      if ((await fs.stat(sourcePath)).isDirectory()) {
        await processDirectory(path.join(subdir, file));
        continue;
      }

      const targetPath = substitute(path.join(subdir, file), variables);
      const input = await fs.readFile(sourcePath, 'utf-8');
      const output = substitute(input.toString(), variables);
      await fs.mkdir(path.dirname(targetPath), { recursive: true });
      await fs.writeFile(targetPath, output);
    }
  }

  async function executePreHook() {
    if (!hooks?.pre) { return; }
    await Promise.resolve(hooks.pre(variables));
  }

  async function executePostHook() {
    if (!hooks?.post) { return; }
    await Promise.resolve(hooks.post(variables));
  }

  function loadHooks() {
    try {
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      return require(path.join(sourceDir, hooksFile));
    } catch {
      return undefined;
    }
  }
}

