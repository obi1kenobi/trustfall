"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.iterify = void 0;
function iterify(obj) {
    obj[Symbol.iterator] = function () {
        return this;
    };
}
exports.iterify = iterify;
