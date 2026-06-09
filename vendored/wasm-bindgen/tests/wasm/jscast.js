class JsCast1 {
  constructor() {
    this.val = 1;
  }
  myval() { return this.val; }
}

class JsCast2 {
}

class JsCast3 extends JsCast1 {
  constructor() {
    super();
    this.val = 3;
  }
}

class JsCast4 extends JsCast3 {
  constructor() {
    super();
    this.val = 4;
  }
}

export { JsCast1 };
export { JsCast2 };
export { JsCast3 };
export { JsCast4 };
