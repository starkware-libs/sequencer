"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
require("@jsii/check-node/run");
const node_fs_1 = require("node:fs");
const path = require("node:path");
const yargs = require("yargs");
const convert_1 = require("./commands/convert");
const coverage_1 = require("./commands/coverage");
const extract_1 = require("./commands/extract");
const infuse_1 = require("./commands/infuse");
const read_1 = require("./commands/read");
const transliterate_1 = require("./commands/transliterate");
const trim_cache_1 = require("./commands/trim-cache");
const index_1 = require("./index");
const languages_1 = require("./languages");
const logging = require("./logging");
const support_1 = require("./support");
const util_1 = require("./util");
async function main() {
    await (0, support_1.emitSupportPolicyInformation)();
    const argv = yargs
        .usage('$0 <cmd> [args]')
        .option('verbose', {
        alias: 'v',
        type: 'boolean',
        desc: 'Increase logging verbosity',
        count: true,
        default: 0,
    })
        .command('snippet FILE', 'Translate a single snippet', (command) => command
        .positional('FILE', {
        type: 'string',
        describe: 'The file to translate (leave out for stdin)',
    })
        .option('language', {
        type: 'string',
        describe: 'Language ID to transliterate to',
        choices: Array.from(new Set(Object.values(languages_1.TargetLanguage))),
    })
        .option('python', {
        alias: 'p',
        boolean: true,
        deprecated: true,
        description: 'Translate snippets to Python. Use --language python instead.',
    }), wrapHandler(async (args) => {
        const result = (0, index_1.translateTypeScript)(await makeFileSource(args.FILE ?? '-', 'stdin.ts'), makeVisitor(args));
        handleSingleResult(result);
    }))
        .command('markdown FILE', 'Translate a MarkDown file', (command) => command
        .positional('FILE', {
        type: 'string',
        describe: 'The file to translate (leave out for stdin)',
    })
        .option('language', {
        type: 'string',
        describe: 'Language ID to transliterate to',
        choices: Array.from(new Set(Object.values(languages_1.TargetLanguage))),
    })
        .option('python', {
        alias: 'p',
        boolean: true,
        deprecated: true,
        description: 'Translate snippets to Python. Use --language python instead.',
    }), wrapHandler(async (args) => {
        const result = (0, convert_1.translateMarkdown)(await makeFileSource(args.FILE ?? '-', 'stdin.md'), makeVisitor(args));
        handleSingleResult(result);
    }))
        .command('infuse [ASSEMBLY..]', '(EXPERIMENTAL) mutates one or more assemblies by adding documentation examples to top-level types', (command) => command
        .positional('ASSEMBLY', {
        type: 'string',
        array: true,
        default: [],
        describe: 'Assembly or directory to mutate',
    })
        .option('log-file', {
        alias: 'l',
        type: 'string',
        describe: 'Output file to store logging results. Ignored if -log is not true',
        default: infuse_1.DEFAULT_INFUSION_RESULTS_NAME,
    })
        .option('cache-from', {
        alias: 'C',
        type: 'string',
        // eslint-disable-next-line prettier/prettier
        describe: 'Reuse translations from the given tablet file if the snippet and type definitions did not change',
        requiresArg: true,
        default: undefined,
    })
        .option('cache-to', {
        alias: 'o',
        type: 'string',
        describe: 'Append all translated snippets to the given tablet file',
        requiresArg: true,
        default: undefined,
    })
        .option('cache', {
        alias: 'k',
        type: 'string',
        describe: 'Alias for --cache-from and --cache-to together',
        requiresArg: true,
        default: undefined,
    })
        .conflicts('cache', 'cache-from')
        .conflicts('cache', 'cache-to'), wrapHandler(async (args) => {
        const absAssemblies = (args.ASSEMBLY.length > 0 ? args.ASSEMBLY : ['.']).map((x) => path.resolve(x));
        const absCacheFrom = (0, util_1.fmap)(args.cache ?? args['cache-from'], path.resolve);
        const absCacheTo = (0, util_1.fmap)(args.cache ?? args['cache-to'], path.resolve);
        const result = await (0, infuse_1.infuse)(absAssemblies, {
            logFile: args['log-file'],
            cacheToFile: absCacheTo,
            cacheFromFile: absCacheFrom,
        });
        let totalTypes = 0;
        let insertedExamples = 0;
        for (const [directory, map] of Object.entries(result.coverageResults)) {
            const commonName = directory.split('/').pop();
            const newCoverage = roundPercentage(map.typesWithInsertedExamples / map.types);
            process.stdout.write(`${commonName}: Added ${map.typesWithInsertedExamples} examples to ${map.types} types.\n`);
            process.stdout.write(`${commonName}: New coverage: ${newCoverage}%.\n`);
            insertedExamples += map.typesWithInsertedExamples;
            totalTypes += map.types;
        }
        const newCoverage = roundPercentage(insertedExamples / totalTypes);
        process.stdout.write(`\n\nFinal Stats:\nNew coverage: ${newCoverage}%.\n`);
    }))
        .command(['extract [ASSEMBLY..]', '$0 [ASSEMBLY..]'], 'Extract code snippets from one or more assemblies into language tablets', (command) => command
        .positional('ASSEMBLY', {
        type: 'string',
        array: true,
        default: [],
        describe: 'Assembly or directory to extract from',
    })
        .option('output', {
        type: 'string',
        describe: 'Additional output file where to store translated samples (deprecated, alias for --cache-to)',
        requiresArg: true,
        default: undefined,
    })
        .option('compile', {
        alias: 'c',
        type: 'boolean',
        describe: 'Try compiling (on by default, use --no-compile to switch off)',
        default: true,
    })
        .option('directory', {
        alias: 'd',
        type: 'string',
        describe: 'Working directory (for require() etc)',
    })
        .option('include', {
        alias: 'i',
        type: 'string',
        array: true,
        describe: 'Extract only snippets with given ids',
        default: [],
    })
        .option('infuse', {
        type: 'boolean',
        describe: 'bundle this command with the infuse command',
        default: false,
    })
        .option('fail', {
        alias: 'f',
        type: 'boolean',
        describe: 'Fail if there are compilation errors',
        default: false,
    })
        .option('validate-assemblies', {
        type: 'boolean',
        describe: 'Whether to validate loaded assemblies or not (this can be slow)',
        default: false,
    })
        .option('cache-from', {
        alias: 'C',
        type: 'string',
        // eslint-disable-next-line prettier/prettier
        describe: 'Reuse translations from the given tablet file if the snippet and type definitions did not change',
        requiresArg: true,
        default: undefined,
    })
        .option('cache-to', {
        alias: 'o',
        type: 'string',
        describe: 'Append all translated snippets to the given tablet file',
        requiresArg: true,
        default: undefined,
    })
        .conflicts('cache-to', 'output')
        .option('cache', {
        alias: 'k',
        type: 'string',
        describe: 'Alias for --cache-from and --cache-to together',
        requiresArg: true,
        default: undefined,
    })
        .conflicts('cache', 'cache-from')
        .conflicts('cache', 'cache-to')
        .option('trim-cache', {
        alias: 'T',
        type: 'boolean',
        describe: 'Remove translations that are not referenced by any of the assemblies anymore from the cache',
    })
        .option('strict', {
        alias: 'S',
        type: 'boolean',
        describe: 'Require all code samples compile, and fail if one does not. Strict mode always enables --compile and --fail',
        default: false,
    })
        .options('loose', {
        alias: 'l',
        describe: 'Ignore missing fixtures and literate markdown files instead of failing',
        type: 'boolean',
    })
        .options('compress-tablet', {
        alias: 'z',
        type: 'boolean',
        describe: 'Compress the implicit tablet file',
        default: false,
    })
        .options('compress-cache', {
        type: 'boolean',
        describe: 'Compress the cache-to file',
        default: false,
    })
        .options('cleanup', {
        type: 'boolean',
        describe: 'Clean up temporary directories',
        default: true,
    })
        .conflicts('loose', 'strict')
        .conflicts('loose', 'fail'), wrapHandler(async (args) => {
        // `--strict` is short for `--compile --fail`, and we'll override those even if they're set to `false`, such as
        // using `--no-(compile|fail)`, because yargs does not quite give us a better option that does not hurt CX.
        if (args.strict) {
            args.compile = args.c = true;
            args.fail = args.f = true;
        }
        const absAssemblies = (args.ASSEMBLY.length > 0 ? args.ASSEMBLY : ['.']).map((x) => path.resolve(x));
        const absCacheFrom = (0, util_1.fmap)(args.cache ?? args['cache-from'], path.resolve);
        const absCacheTo = (0, util_1.fmap)(args.cache ?? args['cache-to'] ?? args.output, path.resolve);
        const extractOptions = {
            compilationDirectory: args.directory,
            includeCompilerDiagnostics: !!args.compile,
            validateAssemblies: args['validate-assemblies'],
            only: args.include,
            cacheFromFile: absCacheFrom,
            cacheToFile: absCacheTo,
            trimCache: args['trim-cache'],
            loose: args.loose,
            compressTablet: args['compress-tablet'],
            compressCacheToFile: args['compress-cache'],
            cleanup: args.cleanup,
        };
        const result = args.infuse
            ? await (0, extract_1.extractAndInfuse)(absAssemblies, extractOptions)
            : await (0, extract_1.extractSnippets)(absAssemblies, extractOptions);
        handleDiagnostics(result.diagnostics, args.fail, result.tablet.count);
    }))
        .command('transliterate [ASSEMBLY..]', '(EXPERIMENTAL) Transliterates the designated assemblies', (command) => command
        .positional('ASSEMBLY', {
        type: 'string',
        array: true,
        default: [],
        required: true,
        describe: 'Assembly to transliterate',
    })
        .option('language', {
        type: 'string',
        array: true,
        default: [],
        describe: 'Language ID to transliterate to',
    })
        .options('strict', {
        alias: 's',
        conflicts: 'loose',
        describe: 'Fail if an example that needs live transliteration fails to compile (which could cause incorrect transpilation results)',
        type: 'boolean',
    })
        .options('loose', {
        alias: 'l',
        conflicts: 'strict',
        describe: 'Ignore missing fixtures and literate markdown files instead of failing',
        type: 'boolean',
    })
        .option('tablet', {
        alias: 't',
        type: 'string',
        describe: 'Language tablet containing pre-translated code examples to use (these are generated by the `extract` command)',
    }), wrapHandler((args) => {
        const assemblies = (args.ASSEMBLY.length > 0 ? args.ASSEMBLY : ['.']).map((dir) => path.resolve(process.cwd(), dir));
        const languages = args.language.length > 0
            ? args.language
                .map((lang) => lang.toUpperCase())
                .map((lang) => {
                const target = Object.entries(languages_1.TargetLanguage).find(([k]) => k === lang)?.[1];
                if (target == null) {
                    throw new Error(`Unknown target language: ${lang}. Expected one of ${Object.keys(languages_1.TargetLanguage).join(', ')}`);
                }
                return target;
            })
            : Object.values(languages_1.TargetLanguage);
        return (0, transliterate_1.transliterateAssembly)(assemblies, languages, args);
    }))
        .command('trim-cache <TABLET> [ASSEMBLY..]', 'Retain only those snippets in the cache which occur in one of the given assemblies', (command) => command
        .positional('TABLET', {
        type: 'string',
        required: true,
        describe: 'Language tablet to trim',
    })
        .positional('ASSEMBLY', {
        type: 'string',
        array: true,
        default: [],
        describe: 'Assembly or directory to search',
    })
        .demandOption('TABLET'), wrapHandler(async (args) => {
        await (0, trim_cache_1.trimCache)({
            cacheFile: args.TABLET,
            assemblyLocations: args.ASSEMBLY,
        });
    }))
        .command('coverage [ASSEMBLY..]', 'Check the translation coverage of implicit tablets for the given assemblies', (command) => command.positional('ASSEMBLY', {
        type: 'string',
        array: true,
        default: ['.'],
        describe: 'Assembly or directory to search',
    }), wrapHandler(async (args) => {
        const absAssemblies = (args.ASSEMBLY.length > 0 ? args.ASSEMBLY : ['.']).map((x) => path.resolve(x));
        await (0, coverage_1.checkCoverage)(absAssemblies);
    }))
        .command('read <TABLET> [KEY] [LANGUAGE]', 'Display snippets in a language tablet file', (command) => command
        .positional('TABLET', {
        type: 'string',
        required: true,
        describe: 'Language tablet to read',
    })
        .positional('KEY', {
        type: 'string',
        describe: 'Snippet key to read',
    })
        .positional('LANGUAGE', {
        type: 'string',
        describe: 'Language ID to read',
    })
        .demandOption('TABLET'), wrapHandler(async (args) => {
        await (0, read_1.readTablet)(args.TABLET, args.KEY, args.LANGUAGE);
    }))
        .command('configure-strict [PACKAGE]', "Enables strict mode for a package's assembly", (command) => command.positional('PACKAGE', {
        type: 'string',
        describe: 'The path to the package to configure',
        required: false,
        default: '.',
        normalize: true,
    }), wrapHandler(async (args) => {
        const packageJsonPath = (await node_fs_1.promises.stat(args.PACKAGE)).isDirectory()
            ? path.join(args.PACKAGE, 'package.json')
            : args.PACKAGE;
        const packageJson = JSON.parse(await node_fs_1.promises.readFile(packageJsonPath, 'utf-8'));
        if (packageJson.jsii == null) {
            console.error(`The package in ${args.PACKAGE} does not have a jsii configuration! You can set it up using jsii-config.`);
            process.exitCode = 1;
            return Promise.resolve();
        }
        if (packageJson.jsii.metadata?.jsii?.rosetta?.strict) {
            // Nothing to do - it's already configured, so we assert idempotent success!
            return Promise.resolve();
        }
        const md = (packageJson.jsii.metadata = packageJson.jsii.metadata ?? {});
        const mdJsii = (md.jsii = md.jsii ?? {});
        const mdRosetta = (mdJsii.rosetta = mdJsii.rosetta ?? {});
        mdRosetta.strict = true;
        return node_fs_1.promises.writeFile(packageJsonPath, JSON.stringify(packageJson, null, 2));
    }))
        .demandCommand()
        .help()
        .strict() // Error on wrong command
        // eslint-disable-next-line @typescript-eslint/no-require-imports,@typescript-eslint/no-var-requires
        .version(require('../package.json').version)
        .showHelpOnFail(false).argv;
    // Evaluating .argv triggers the parsing but the command gets implicitly executed,
    // so we don't need the output.
    Array.isArray(argv);
}
/**
 * Wrap a command's handler with standard pre- and post-work
 */
function wrapHandler(handler) {
    return (argv) => {
        logging.configure({ level: argv.verbose !== undefined ? argv.verbose : 0 });
        handler(argv).catch((e) => {
            logging.error(e.message);
            logging.error(e.stack);
            process.exitCode = 1;
        });
    };
}
function makeVisitor(args) {
    if (args.python != null && args.language == null) {
        args.language = 'python';
    }
    return (0, languages_1.getVisitorFromLanguage)(args.language);
}
async function makeFileSource(fileName, stdinName) {
    if (fileName === '-') {
        return {
            contents: await readStdin(),
            fileName: stdinName,
        };
    }
    return {
        contents: await node_fs_1.promises.readFile(fileName, { encoding: 'utf-8' }),
        fileName: fileName,
    };
}
async function readStdin() {
    process.stdin.setEncoding('utf8');
    const parts = [];
    return new Promise((resolve, reject) => {
        process.stdin.on('readable', () => {
            const chunk = process.stdin.read();
            if (chunk !== null) {
                parts.push(Buffer.from(chunk));
            }
        });
        process.stdin.on('error', reject);
        process.stdin.on('end', () => resolve(Buffer.concat(parts).toString('utf-8')));
    });
}
function handleSingleResult(result) {
    process.stdout.write(`${result.translation}\n`);
    // For a single result, we always request implicit failure.
    handleDiagnostics(result.diagnostics, 'implicit');
}
/**
 * Print diagnostics and set exit code
 *
 * 'fail' is whether or not the user passed '--fail' for commands that accept
 * it, or 'implicit' for commands that should always fail. 'implicit' will be
 * treated as 'fail=true, but will not print to the user that the '--fail' is
 * set (because for this particular command that switch does not exist and so it
 * would be confusing).
 */
function handleDiagnostics(diagnostics, fail, snippetCount = 1) {
    if (fail !== false) {
        // Fail on any diagnostic
        if (diagnostics.length > 0) {
            (0, util_1.printDiagnostics)(diagnostics, process.stderr, process.stderr.isTTY);
            logging.error([
                `${diagnostics.length} diagnostics encountered in ${snippetCount} snippets`,
                ...(fail === true ? ["(running with '--fail')"] : []),
            ].join(' '));
            process.exitCode = 1;
        }
        return;
    }
    // Otherwise fail only on strict diagnostics. If we have strict diagnostics, print only those
    // (so it's very clear what is failing the build), otherwise print everything.
    const strictDiagnostics = diagnostics.filter((diag) => diag.isFromStrictAssembly);
    if (strictDiagnostics.length > 0) {
        (0, util_1.printDiagnostics)(strictDiagnostics, process.stderr, process.stderr.isTTY);
        const remaining = diagnostics.length - strictDiagnostics.length;
        logging.warn([
            `${strictDiagnostics.length} diagnostics from assemblies with 'strict' mode on`,
            ...(remaining > 0 ? [`(and ${remaining} more non-strict diagnostics)`] : []),
        ].join(' '));
        process.exitCode = 1;
        return;
    }
    if (diagnostics.length > 0) {
        (0, util_1.printDiagnostics)(diagnostics, process.stderr, process.stderr.isTTY);
        logging.warn(`${diagnostics.length} diagnostics encountered in ${snippetCount} snippets`);
    }
}
/**
 * Rounds a decimal number to two decimal points.
 * The function is useful for fractions that need to be outputted as percentages.
 */
function roundPercentage(num) {
    return Math.round(10000 * num) / 100;
}
main().catch((cause) => {
    console.error(cause);
    process.exitCode = -1;
});
//# sourceMappingURL=main.js.map