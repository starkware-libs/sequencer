/**
 * YAML utilities.
 */
export declare class Yaml {
    /**
     * @deprecated use `stringify(doc[, doc, ...])`
     */
    static formatObjects(docs: any[]): string;
    /**
     * Saves a set of objects as a multi-document YAML file.
     * @param filePath The output path
     * @param docs The set of objects
     */
    static save(filePath: string, docs: any[]): void;
    /**
     * Stringify a document (or multiple documents) into YAML
     *
     * We convert undefined values to null, but ignore any documents that are
     * undefined.
     *
     * @param docs A set of objects to convert to YAML
     * @returns a YAML string. Multiple docs are separated by `---`.
     */
    static stringify(...docs: any[]): string;
    /**
     * Saves a set of YAML documents into a temp file (in /tmp)
     *
     * @returns the path to the temporary file
     * @param docs the set of documents to save
     */
    static tmp(docs: any[]): string;
    /**
     * Downloads a set of YAML documents (k8s manifest for example) from a URL or
     * a file and returns them as javascript objects.
     *
     * Empty documents are filtered out.
     *
     * @param urlOrFile a URL of a file path to load from
     * @returns an array of objects, each represents a document inside the YAML
     */
    static load(urlOrFile: string): any[];
    /**
     * Utility class.
     */
    private constructor();
}
