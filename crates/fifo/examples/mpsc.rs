use tesi_fifo::channel;

// This example shows how to use a FIFO to synchronize an arbitary number of producer threads with a single consumer thread.
fn main() {
    // Create some channels.
    let capacity = 4096;
    let num_producers = 4;
    let mut readers = Vec::with_capacity(num_producers);
    let mut writers = Vec::with_capacity(num_producers);
    for _ in 0..num_producers {
        let (writer, reader) = channel::<u8>(capacity, None, || 0);
        readers.push(reader);
        writers.push(writer);
    }

    // Spawn a bunch of producer threads, writing to fifos.
    for (index, mut writer) in writers.into_iter().enumerate() {
        std::thread::spawn(move || {
            let body = vec![index as u8; capacity];
            for mut chunk in body.chunks_exact(32) {
                std::thread::sleep(std::time::Duration::from_millis(1));
                while !chunk.is_empty() {
                    let Some(mut txn) = writer.write(body.len()) else {
                        return;
                    };
                    if txn.is_empty() {
                        std::hint::spin_loop();
                    }
                    let len = txn.len().min(chunk.len());
                    txn[0..len].copy_from_slice(&chunk[0..len]);
                    chunk = &chunk[len..];
                }
            }
        });
    }

    let mut outp = Vec::new();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(1));
        // We need to batch all the chunks together, which could be of different sizes. We can do this by peeking at every element.
        let Some(chunk_size) = readers
            .iter()
            .map(|reader| reader.available())
            .min()
        else {
            break;
        };

        let len = outp.len();
        outp.resize_with(len + chunk_size, || 0);
        for reader in &mut readers {
            let Some(txn) = reader.read() else {
                continue;
            };
            for (idx, byte) in txn.iter().copied().take(chunk_size).enumerate() {
                outp[len + idx] += byte;
            }
            txn.commit_n(chunk_size);
        }
    }
    eprintln!("{outp:#?}");
}
