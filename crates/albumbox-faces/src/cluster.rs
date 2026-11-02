//! Grouping embeddings of unnamed faces and matching them to known persons.

/// Two faces with at least this cosine similarity are put in the same group.
pub const CLUSTER_THRESHOLD: f32 = 0.45;
/// A face this similar to a person's average is suggested as that person.
pub const SUGGEST_THRESHOLD: f32 = 0.42;

pub fn normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        v.iter_mut().for_each(|x| *x /= n);
    }
}

/// Cosine similarity of two normalized vectors.
pub fn similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Normalized mean of embeddings (a person's or group's "average face").
pub fn centroid<'a>(vectors: impl IntoIterator<Item = &'a [f32]>) -> Option<Vec<f32>> {
    let mut sum: Option<Vec<f32>> = None;
    for v in vectors {
        match &mut sum {
            None => sum = Some(v.to_vec()),
            Some(s) => s.iter_mut().zip(v).for_each(|(a, b)| *a += b),
        }
    }
    sum.map(|mut s| {
        normalize(&mut s);
        s
    })
}

/// Best matching person for an embedding, if similar enough.
pub fn best_person(embedding: &[f32], persons: &[(i64, Vec<f32>)], threshold: f32) -> Option<(i64, f32)> {
    persons
        .iter()
        .map(|(id, c)| (*id, similarity(embedding, c)))
        .filter(|(_, s)| *s >= threshold)
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// Greedy centroid clustering followed by one re-assignment pass.
/// Returns a group index for every input (same order). Deterministic for a given order.
pub fn cluster(embeddings: &[&[f32]], threshold: f32) -> Vec<usize> {
    let mut sums: Vec<Vec<f32>> = Vec::new();
    let mut centers: Vec<Vec<f32>> = Vec::new();
    let mut labels = Vec::with_capacity(embeddings.len());
    for e in embeddings {
        let best = centers
            .iter()
            .enumerate()
            .map(|(i, c)| (i, similarity(e, c)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((i, s)) if s >= threshold => {
                sums[i].iter_mut().zip(e.iter()).for_each(|(a, b)| *a += b);
                let mut c = sums[i].clone();
                normalize(&mut c);
                centers[i] = c;
                labels.push(i);
            }
            _ => {
                sums.push(e.to_vec());
                centers.push(e.to_vec());
                labels.push(centers.len() - 1);
            }
        }
    }
    // Re-assign with the final centers so early faces are not stuck in a worse group.
    for (label, e) in labels.iter_mut().zip(embeddings) {
        if let Some((i, s)) = centers.iter().enumerate().map(|(i, c)| (i, similarity(e, c))).max_by(|a, b| a.1.total_cmp(&b.1)) {
            if s >= threshold {
                *label = i;
            }
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(seed: &[f32]) -> Vec<f32> {
        let mut x = seed.to_vec();
        normalize(&mut x);
        x
    }

    #[test]
    fn groups_similar_vectors() {
        let a1 = v(&[1.0, 0.1, 0.0]);
        let a2 = v(&[0.9, 0.2, 0.0]);
        let b1 = v(&[0.0, 0.1, 1.0]);
        let b2 = v(&[0.1, 0.0, 0.95]);
        let labels = cluster(&[&a1, &b1, &a2, &b2], CLUSTER_THRESHOLD);
        assert_eq!(labels[0], labels[2]);
        assert_eq!(labels[1], labels[3]);
        assert_ne!(labels[0], labels[1]);

        let persons = vec![(7, centroid([a1.as_slice(), a2.as_slice()]).unwrap())];
        assert_eq!(best_person(&a2, &persons, SUGGEST_THRESHOLD).map(|p| p.0), Some(7));
        assert_eq!(best_person(&b1, &persons, SUGGEST_THRESHOLD), None);
    }
}
