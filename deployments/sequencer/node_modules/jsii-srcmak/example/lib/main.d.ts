/**
 * Math operands
 */
export interface Operands {
    /**
     * Left-hand side operand
     */
    readonly lhs: number;
    /**
     * Right-hand side operand
     */
    readonly rhs: number;
}
/**
 * A sophisticaed multi-language calculator
 */
export declare class Calculator {
    /**
     * Adds the two operands
     * @param ops operands
     */
    add(ops: Operands): number;
    /**
     * Subtracts the two operands
     * @param ops operands
     */
    sub(ops: Operands): number;
    /**
     * Multiplies the two operands
     * @param ops operands
     */
    mul(ops: Operands): number;
}
