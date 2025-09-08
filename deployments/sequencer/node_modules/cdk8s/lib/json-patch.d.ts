/**
 * Utility for applying RFC-6902 JSON-Patch to a document.
 *
 * Use the the `JsonPatch.apply(doc, ...ops)` function to apply a set of
 * operations to a JSON document and return the result.
 *
 * Operations can be created using the factory methods `JsonPatch.add()`,
 * `JsonPatch.remove()`, etc.
 *
 * @example
 *
 *const output = JsonPatch.apply(input,
 *  JsonPatch.replace('/world/hi/there', 'goodbye'),
 *  JsonPatch.add('/world/foo/', 'boom'),
 *  JsonPatch.remove('/hello'));
 *
 */
export declare class JsonPatch {
    private readonly operation;
    /**
     * Applies a set of JSON-Patch (RFC-6902) operations to `document` and returns the result.
     * @param document The document to patch
     * @param ops The operations to apply
     * @returns The result document
     */
    static apply(document: any, ...ops: JsonPatch[]): any;
    /**
     * Adds a value to an object or inserts it into an array. In the case of an
     * array, the value is inserted before the given index. The - character can be
     * used instead of an index to insert at the end of an array.
     *
     * @example JsonPatch.add('/biscuits/1', { "name": "Ginger Nut" })
     */
    static add(path: string, value: any): JsonPatch;
    /**
     * Removes a value from an object or array.
     *
     * @example JsonPatch.remove('/biscuits')
     * @example JsonPatch.remove('/biscuits/0')
     */
    static remove(path: string): JsonPatch;
    /**
     * Replaces a value. Equivalent to a “remove” followed by an “add”.
     *
     * @example JsonPatch.replace('/biscuits/0/name', 'Chocolate Digestive')
     */
    static replace(path: string, value: any): JsonPatch;
    /**
     * Copies a value from one location to another within the JSON document. Both
     * from and path are JSON Pointers.
     *
     * @example JsonPatch.copy('/biscuits/0', '/best_biscuit')
     */
    static copy(from: string, path: string): JsonPatch;
    /**
     * Moves a value from one location to the other. Both from and path are JSON Pointers.
     *
     * @example JsonPatch.move('/biscuits', '/cookies')
     */
    static move(from: string, path: string): JsonPatch;
    /**
     * Tests that the specified value is set in the document. If the test fails,
     * then the patch as a whole should not apply.
     *
     * @example JsonPatch.test('/best_biscuit/name', 'Choco Leibniz')
     */
    static test(path: string, value: any): JsonPatch;
    private constructor();
    /**
     * Returns the JSON representation of this JSON patch operation.
     *
     * @internal
     */
    _toJson(): any;
}
