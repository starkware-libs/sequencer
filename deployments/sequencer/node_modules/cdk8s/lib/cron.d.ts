/**
 * Represents a cron schedule
 */
export declare class Cron {
    /**
     * Create a cron schedule which runs every minute
     */
    static everyMinute(): Cron;
    /**
     * Create a cron schedule which runs every hour
     */
    static hourly(): Cron;
    /**
     * Create a cron schedule which runs every day at midnight
     */
    static daily(): Cron;
    /**
     * Create a cron schedule which runs every week on Sunday
     */
    static weekly(): Cron;
    /**
     * Create a cron schedule which runs first day of every month
     */
    static monthly(): Cron;
    /**
     * Create a cron schedule which runs first day of January every year
     */
    static annually(): Cron;
    /**
     * Create a custom cron schedule from a set of cron fields
     */
    static schedule(options: CronOptions): Cron;
    /**
     * Retrieve the expression for this schedule
     */
    readonly expressionString: string;
    constructor(cronOptions?: CronOptions);
}
/**
 * Options to configure a cron expression
 *
 * All fields are strings so you can use complex expressions. Absence of
 * a field implies '*'
 */
export interface CronOptions {
    /**
     * The minute to run this rule at
     *
     * @default - Every minute
     */
    readonly minute?: string;
    /**
     * The hour to run this rule at
     *
     * @default - Every hour
     */
    readonly hour?: string;
    /**
     * The day of the month to run this rule at
     *
     * @default - Every day of the month
     */
    readonly day?: string;
    /**
     * The month to run this rule at
     *
     * @default - Every month
     */
    readonly month?: string;
    /**
     * The day of the week to run this rule at
     *
     * @default - Any day of the week
     */
    readonly weekDay?: string;
}
