export default function iterify(obj: any) {
    obj[Symbol.iterator] = function () {
        return this;
    };
}
