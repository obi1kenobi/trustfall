export interface SendableSyncContext {
  contentBuffer: SharedArrayBuffer;
  ctrlBuffer: SharedArrayBuffer;
}

export class SyncContext {
  contentBuffer: SharedArrayBuffer;
  ctrlBuffer: SharedArrayBuffer;

  ctrl: Int32Array;

  static CTRL_BUFFER_LENGTH = 12;
  static CONTENT_BUFFER_LENGTH = 4096;

  static CTRL_OFFSET_STATE = 0;
  static CTRL_OFFSET_CURRENT_WRITE_SIZE = 1;
  static CTRL_OFFSET_TOTAL_SIZE = 2;

  static STATE_WAITING_FOR_DATA = 128;
  static STATE_DONE = 1;
  static STATE_BUFFER_FULL = 2;
  static STATE_TERMINATED_ERROR = 3;

  constructor({ ctrlBuffer, contentBuffer }: SendableSyncContext) {
    this.ctrlBuffer = ctrlBuffer;
    this.contentBuffer = contentBuffer;

    this.ctrl = new Int32Array(this.ctrlBuffer);
    Atomics.store(this.ctrl, SyncContext.CTRL_OFFSET_STATE, SyncContext.STATE_WAITING_FOR_DATA);
  }

  static makeDefault(): SyncContext {
    const ctrlBuffer = new SharedArrayBuffer(SyncContext.CTRL_BUFFER_LENGTH);
    const contentBuffer = new SharedArrayBuffer(SyncContext.CONTENT_BUFFER_LENGTH);
    return new SyncContext({ ctrlBuffer, contentBuffer });
  }

  makeSendable(): SendableSyncContext {
    return {
      ctrlBuffer: this.ctrlBuffer,
      contentBuffer: this.contentBuffer,
    };
  }

  sendError(reason: string): void {
    const array = new TextEncoder().encode(reason);
    this.sendInner(array, true);
  }

  send(array: Uint8Array): void {
    this.sendInner(array, false);
  }

  private sendInner(array: Uint8Array, isError: boolean): void {
    let position = 0;
    let remainingBytes = array.byteLength;

    Atomics.store(this.ctrl, SyncContext.CTRL_OFFSET_TOTAL_SIZE, remainingBytes);
    const writeBuffer = new Uint8Array(this.contentBuffer);

    while (remainingBytes > this.contentBuffer.byteLength) {
      // Write a portion of the data, since the remaining size is larger than our buffer.
      const temp = array.slice(position, position + writeBuffer.byteLength);
      writeBuffer.set(temp);
      position += writeBuffer.byteLength;
      remainingBytes -= writeBuffer.byteLength;

      Atomics.store(this.ctrl, SyncContext.CTRL_OFFSET_CURRENT_WRITE_SIZE, writeBuffer.byteLength);
      Atomics.store(this.ctrl, SyncContext.CTRL_OFFSET_STATE, SyncContext.STATE_BUFFER_FULL);
      Atomics.notify(this.ctrl, SyncContext.CTRL_OFFSET_STATE);
      Atomics.wait(this.ctrl, SyncContext.CTRL_OFFSET_STATE, SyncContext.STATE_BUFFER_FULL);
    }

    // Write the remaining data, which will completely fit in our buffer.
    const temp = array.slice(position);
    writeBuffer.set(temp);

    Atomics.store(this.ctrl, SyncContext.CTRL_OFFSET_CURRENT_WRITE_SIZE, remainingBytes);

    if (isError) {
      Atomics.store(this.ctrl, SyncContext.CTRL_OFFSET_STATE, SyncContext.STATE_TERMINATED_ERROR);
    } else {
      Atomics.store(this.ctrl, SyncContext.CTRL_OFFSET_STATE, SyncContext.STATE_DONE);
    }
    Atomics.notify(this.ctrl, SyncContext.CTRL_OFFSET_STATE);
  }

  receive(): Uint8Array {
    Atomics.wait(this.ctrl, SyncContext.CTRL_OFFSET_STATE, SyncContext.STATE_WAITING_FOR_DATA);

    const totalLength = Atomics.load(this.ctrl, SyncContext.CTRL_OFFSET_TOTAL_SIZE);
    const output = new Uint8Array(totalLength);

    let writePosition = 0;

    let currentState = Atomics.load(this.ctrl, SyncContext.CTRL_OFFSET_STATE);

    while (currentState == SyncContext.STATE_BUFFER_FULL) {
      // Receiving a portion of the full output.
      const readLength = Atomics.load(this.ctrl, SyncContext.CTRL_OFFSET_CURRENT_WRITE_SIZE);
      const temp = this.contentBuffer.slice(0, readLength);
      output.set(new Uint8Array(temp), writePosition);
      writePosition += readLength;

      Atomics.store(this.ctrl, SyncContext.CTRL_OFFSET_STATE, SyncContext.STATE_WAITING_FOR_DATA);
      Atomics.notify(this.ctrl, SyncContext.CTRL_OFFSET_STATE);
      Atomics.wait(this.ctrl, SyncContext.CTRL_OFFSET_STATE, SyncContext.STATE_WAITING_FOR_DATA);

      currentState = Atomics.load(this.ctrl, SyncContext.CTRL_OFFSET_STATE);
    }

    // Receiving the last of the output data.
    const readLength = Atomics.load(this.ctrl, SyncContext.CTRL_OFFSET_CURRENT_WRITE_SIZE);
    const temp = this.contentBuffer.slice(0, readLength);
    output.set(new Uint8Array(temp), writePosition);

    // Return the state to its initial value.
    Atomics.store(this.ctrl, SyncContext.CTRL_OFFSET_STATE, SyncContext.STATE_WAITING_FOR_DATA);

    if (currentState === SyncContext.STATE_DONE) {
      return output;
    } else if (currentState === SyncContext.STATE_TERMINATED_ERROR) {
      throw new Error(new TextDecoder().decode(output));
    } else {
      throw new Error(`SyncContext: unexpected final state ${currentState}`);
    }
  }
}
