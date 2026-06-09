export const import_me = function() {};

globalThis.webkitMySpecialApi = class {
  foo() { return 123; }
};
globalThis.MySpecialApi2 = class {
  foo() { return 124; }
};
globalThis.bMySpecialApi3 = class {
  foo() { return 125; }
};
