"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.TranslatedSnippet = exports.LanguageTablet = exports.CURRENT_SCHEMA_VERSION = exports.DEFAULT_TABLET_NAME_COMPRESSED = exports.DEFAULT_TABLET_NAME = void 0;
const node_fs_1 = require("node:fs");
const path = require("node:path");
const zlib = require("node:zlib");
const key_1 = require("./key");
const schema_1 = require("./schema");
const json_1 = require("../json");
const logging = require("../logging");
const snippet_1 = require("../snippet");
const util_1 = require("../util");
// eslint-disable-next-line @typescript-eslint/no-require-imports,@typescript-eslint/no-var-requires
const TOOL_VERSION = require('../../package.json').version;
/**
 * The default name of the tablet file
 */
exports.DEFAULT_TABLET_NAME = '.jsii.tabl.json';
/**
 * The default name of the compressed tablet file
 */
exports.DEFAULT_TABLET_NAME_COMPRESSED = '.jsii.tabl.json.gz';
exports.CURRENT_SCHEMA_VERSION = '2';
/**
 * A tablet containing various snippets in multiple languages
 */
class LanguageTablet {
    constructor() {
        /**
         * Whether or not the LanguageTablet was loaded with a compressed source.
         * This gets used to determine if it should be compressed when saved.
         */
        this.compressedSource = false;
        this.snippets = {};
    }
    /**
     * Load a tablet from a file
     */
    static async fromFile(filename) {
        const ret = new LanguageTablet();
        await ret.load(filename);
        return ret;
    }
    /**
     * Load a tablet from a file that may not exist
     *
     * Will return an empty tablet if the file does not exist
     */
    static async fromOptionalFile(filename) {
        const ret = new LanguageTablet();
        if ((0, node_fs_1.existsSync)(filename)) {
            try {
                await ret.load(filename);
            }
            catch (e) {
                logging.warn(`${filename}: ${e}`);
            }
        }
        return ret;
    }
    /**
     * Add one or more snippets to this tablet
     */
    addSnippets(...snippets) {
        for (const snippet of snippets) {
            const existingSnippet = this.snippets[snippet.key];
            this.snippets[snippet.key] = existingSnippet ? existingSnippet.mergeTranslations(snippet) : snippet;
        }
    }
    /**
     * Add one snippet to this tablet
     *
     * @deprecated use addSnippets instead
     */
    addSnippet(snippet) {
        this.addSnippets(snippet);
    }
    get snippetKeys() {
        return Object.keys(this.snippets);
    }
    /**
     * Add all snippets from the given tablets into this one
     */
    addTablets(...tablets) {
        for (const tablet of tablets) {
            for (const snippet of Object.values(tablet.snippets)) {
                this.addSnippet(snippet);
            }
        }
    }
    /**
     * Add all snippets from the given tablet into this one
     *
     * @deprecated Use `addTablets()` instead.
     */
    addTablet(tablet) {
        this.addTablets(tablet);
    }
    tryGetSnippet(key) {
        return this.snippets[key];
    }
    /**
     * Look up a single translation of a source snippet
     *
     * @deprecated Use `lookupTranslationBySource` instead.
     */
    lookup(typeScriptSource, language) {
        return this.lookupTranslationBySource(typeScriptSource, language);
    }
    /**
     * Look up a single translation of a source snippet
     */
    lookupTranslationBySource(typeScriptSource, language) {
        const snippet = this.snippets[(0, key_1.snippetKey)(typeScriptSource)];
        return snippet?.get(language);
    }
    /**
     * Lookup the translated verion of a TypeScript snippet
     */
    lookupBySource(typeScriptSource) {
        return this.snippets[(0, key_1.snippetKey)(typeScriptSource)];
    }
    /**
     * Load the tablet from a file. Will automatically detect if the file is
     * compressed and decompress accordingly.
     */
    async load(filename) {
        let readStream;
        if (await isGzipped(filename)) {
            const gunzip = zlib.createGunzip();
            (0, node_fs_1.createReadStream)(filename).pipe(gunzip, { end: true });
            readStream = gunzip;
            this.compressedSource = true;
        }
        else {
            readStream = (0, node_fs_1.createReadStream)(filename);
        }
        const obj = await (0, json_1.parse)(readStream);
        if (!obj.toolVersion || !obj.snippets) {
            throw new Error(`File '${filename}' does not seem to be a Tablet file`);
        }
        if (obj.version !== exports.CURRENT_SCHEMA_VERSION) {
            // If we're ever changing the schema version in a backwards incompatible way,
            // do upconversion here.
            throw new Error(`Tablet file '${filename}' has schema version '${obj.version}', this program expects '${exports.CURRENT_SCHEMA_VERSION}'`);
        }
        Object.assign(this.snippets, (0, util_1.mapValues)(obj.snippets, TranslatedSnippet.fromSchema));
    }
    get count() {
        return Object.keys(this.snippets).length;
    }
    get translatedSnippets() {
        return Object.values(this.snippets);
    }
    /**
     * Saves the tablet schema to a file. If the compress option is passed, then
     * the schema will be gzipped before writing to the file.
     */
    async save(filename, compress = false) {
        await node_fs_1.promises.mkdir(path.dirname(filename), { recursive: true });
        const writeStream = (0, node_fs_1.createWriteStream)(filename, { flags: 'w' });
        const gzip = compress ? zlib.createGzip() : undefined;
        return (0, json_1.stringify)(this.toSchema(), ...(gzip ? [gzip] : []), writeStream);
    }
    toSchema() {
        return {
            version: exports.CURRENT_SCHEMA_VERSION,
            toolVersion: TOOL_VERSION,
            snippets: (0, util_1.mapValues)(this.snippets, (s) => s.snippet),
        };
    }
}
exports.LanguageTablet = LanguageTablet;
/**
 * Mutable operations on an underlying TranslatedSnippetSchema
 */
class TranslatedSnippet {
    static fromSchema(schema) {
        if (!schema.translations[schema_1.ORIGINAL_SNIPPET_KEY]) {
            throw new Error(`Input schema must have '${schema_1.ORIGINAL_SNIPPET_KEY}' key set in translations`);
        }
        return new TranslatedSnippet(schema);
    }
    static fromTypeScript(original, didCompile) {
        return new TranslatedSnippet({
            translations: {
                [schema_1.ORIGINAL_SNIPPET_KEY]: { source: original.visibleSource, version: '0' },
            },
            didCompile: didCompile,
            location: original.location,
            fullSource: (0, snippet_1.completeSource)(original),
        });
    }
    constructor(snippet) {
        this._snippet = { ...snippet };
        this.snippet = this._snippet;
    }
    get key() {
        if (this._key === undefined) {
            this._key = (0, key_1.snippetKey)(this.asTypescriptSnippet());
        }
        return this._key;
    }
    get originalSource() {
        return {
            source: this.snippet.translations[schema_1.ORIGINAL_SNIPPET_KEY].source,
            language: 'typescript',
            didCompile: this.snippet.didCompile,
        };
    }
    addTranslation(language, translation, version) {
        this.snippet.translations[language] = { source: translation, version };
        return {
            source: translation,
            language,
            didCompile: this.snippet.didCompile,
        };
    }
    fqnsReferenced() {
        return this._snippet.fqnsReferenced ?? [];
    }
    addSyntaxKindCounter(syntaxKindCounter) {
        if (!this._snippet.syntaxKindCounter) {
            this._snippet.syntaxKindCounter = {};
        }
        for (const [key, value] of Object.entries(syntaxKindCounter)) {
            const x = this._snippet.syntaxKindCounter[key] ?? 0;
            this._snippet.syntaxKindCounter[key] = value + x;
        }
    }
    get languages() {
        return Object.keys(this.snippet.translations).filter((x) => x !== schema_1.ORIGINAL_SNIPPET_KEY);
    }
    get(language) {
        const t = this.snippet.translations[language];
        return t && { source: t.source, language, didCompile: this.snippet.didCompile };
    }
    mergeTranslations(other) {
        return new TranslatedSnippet({
            ...this.snippet,
            translations: { ...this.snippet.translations, ...other.snippet.translations },
        });
    }
    withFingerprint(fp) {
        return new TranslatedSnippet({
            ...this.snippet,
            fqnsFingerprint: fp,
        });
    }
    withLocation(location) {
        return new TranslatedSnippet({
            ...this.snippet,
            location,
        });
    }
    toJSON() {
        return this._snippet;
    }
    asTypescriptSnippet() {
        return {
            visibleSource: this.snippet.translations[schema_1.ORIGINAL_SNIPPET_KEY].source,
            location: this.snippet.location,
        };
    }
}
exports.TranslatedSnippet = TranslatedSnippet;
async function isGzipped(filename) {
    const openFile = await node_fs_1.promises.open(filename, 'r');
    try {
        // Assumes that we can always read 3 bytes if there's that many in the file...
        const { bytesRead, buffer } = await openFile.read(Buffer.alloc(4), 0, 3, 0);
        return bytesRead >= 3 && buffer[0] === 0x1f && buffer[1] === 0x8b && buffer[2] === 0x08;
    }
    finally {
        await openFile.close();
    }
}
//# sourceMappingURL=tablets.js.map