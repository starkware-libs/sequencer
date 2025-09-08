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
export class Calculator {
  /**
   * Adds the two operands
   * @param ops operands
   */
  public add(ops: Operands) {
    return ops.lhs + ops.rhs;
  }

  /**
   * Subtracts the two operands
   * @param ops operands
   */
  public sub(ops: Operands) {
    return ops.lhs - ops.rhs;
  }
  
  /**
   * Multiplies the two operands
   * @param ops operands
   */
  public mul(ops: Operands) {
    return ops.lhs * ops.rhs
  }
}
