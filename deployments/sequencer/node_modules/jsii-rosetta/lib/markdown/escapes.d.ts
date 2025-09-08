export interface Escaper {
    /**
     * Escape for use in XML/HTML text
     */
    text(x: string | null): string;
    /**
     * Escape for use in XML/HTML attributes
     */
    attribute(x: string | null): string;
    /**
     * Re-escape a string that has been escaped for text to be escaped for attributes
     *
     * Conceptually this unescapes text back to raw and re-escapes for attributes,
     * but for speed in practice we just do the additional escapes.
     */
    text2attr(x: string | null): string;
}
/**
 * Make a generic XML escaper
 */
export declare function makeXmlEscaper(): Escaper;
/**
 * Make a Java specific escaper
 *
 * This one also escapes '@' because that triggers parsing of comment directives
 * in Java.
 */
export declare function makeJavaEscaper(): Escaper;
//# sourceMappingURL=escapes.d.ts.map