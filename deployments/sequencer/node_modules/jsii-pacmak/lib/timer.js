"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Timers = exports.Timer = void 0;
/**
 * A single timer
 */
class Timer {
    constructor(label) {
        this.label = label;
        this.startTime = Date.now();
    }
    start() {
        this.startTime = Date.now();
    }
    end() {
        this.timeMs = (Date.now() - this.startTime) / 1000;
    }
    isSet() {
        return this.timeMs !== undefined;
    }
    humanTime() {
        if (!this.timeMs) {
            return '???';
        }
        const parts = [];
        let time = this.timeMs;
        if (time > 60) {
            const mins = Math.floor(time / 60);
            parts.push(`${mins}m`);
            time -= mins * 60;
        }
        parts.push(`${time.toFixed(1)}s`);
        return parts.join('');
    }
}
exports.Timer = Timer;
/**
 * A collection of Timers
 */
class Timers {
    constructor() {
        this.timers = [];
    }
    record(label, operation) {
        const timer = this.start(label);
        try {
            const x = operation();
            timer.end();
            return x;
        }
        catch (e) {
            timer.end();
            throw e;
        }
    }
    async recordAsync(label, operation) {
        const timer = this.start(label);
        return operation().finally(() => timer.end());
    }
    start(label) {
        const timer = new Timer(label);
        this.timers.push(timer);
        return timer;
    }
    display() {
        const timers = this.timers.filter((t) => t.isSet());
        timers.sort((a, b) => b.timeMs - a.timeMs);
        return timers.map((t) => `${t.label} (${t.humanTime()})`).join(' | ');
    }
}
exports.Timers = Timers;
//# sourceMappingURL=timer.js.map