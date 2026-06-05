export const MyDurableObject = class MyDurableObject {
  constructor() {}
};

export const MyDurableObjectStub = class MyDurableObjectStub {
  constructor() {}
};

export const DurableObjectNamespace = class DurableObjectNamespace {
  constructor() {}

  getByName(name) {
    return new MyDurableObjectStub();
  }
};
