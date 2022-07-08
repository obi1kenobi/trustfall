
    export function iterify(obj) {
        obj[Symbol.iterator] = function () {
            return this;
        };
    }
