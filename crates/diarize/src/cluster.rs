//! Agglomerative clustering of speaker embeddings (average linkage on cosine distance).

use crate::cosine;

/// Cluster unit vectors. Merging stops when the closest pair of clusters is further apart than
/// `threshold` (cosine distance, 0 = same direction, 2 = opposite) or `max_clusters` would be
/// exceeded the other way round. Clusters whose total `weight` is below `min_weight` are folded
/// into their nearest neighbour afterwards. Returns a label per item, numbered by first
/// appearance.
pub fn agglomerative(items: &[Vec<f32>], weights: &[f32], threshold: f32, max_clusters: usize, min_weight: f32) -> Vec<usize> {
    let n = items.len();
    if n == 0 {
        return Vec::new();
    }
    // Pairwise distances.
    let mut dist = vec![vec![0.0f32; n]; n];
    for i in 0..n {
        for j in i + 1..n {
            let d = 1.0 - cosine(&items[i], &items[j]);
            dist[i][j] = d;
            dist[j][i] = d;
        }
    }
    let mut members: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    let mut alive: Vec<bool> = vec![true; n];
    let avg = |a: &[usize], b: &[usize], dist: &Vec<Vec<f32>>| -> f32 {
        let mut s = 0.0;
        for &i in a {
            for &j in b {
                s += dist[i][j];
            }
        }
        s / (a.len() * b.len()) as f32
    };
    loop {
        let live: Vec<usize> = (0..n).filter(|&i| alive[i]).collect();
        if live.len() <= 1 {
            break;
        }
        let mut best = (f32::MAX, 0, 0);
        for (x, &a) in live.iter().enumerate() {
            for &b in &live[x + 1..] {
                let d = avg(&members[a], &members[b], &dist);
                if d < best.0 {
                    best = (d, a, b);
                }
            }
        }
        if best.0 > threshold && live.len() <= max_clusters {
            break;
        }
        let (_, a, b) = best;
        let moved = std::mem::take(&mut members[b]);
        members[a].extend(moved);
        alive[b] = false;
    }
    // Fold light clusters into their nearest heavy neighbour.
    let weight_of = |m: &[usize]| m.iter().map(|&i| weights.get(i).copied().unwrap_or(1.0)).sum::<f32>();
    loop {
        let live: Vec<usize> = (0..n).filter(|&i| alive[i]).collect();
        if live.len() <= 1 {
            break;
        }
        let Some(&light) = live.iter().filter(|&&c| weight_of(&members[c]) < min_weight).min_by(|&&a, &&b| weight_of(&members[a]).total_cmp(&weight_of(&members[b])))
        else {
            break;
        };
        let target = live
            .iter()
            .filter(|&&c| c != light)
            .min_by(|&&a, &&b| avg(&members[light], &members[a], &dist).total_cmp(&avg(&members[light], &members[b], &dist)))
            .copied();
        let Some(target) = target else { break };
        let moved = std::mem::take(&mut members[light]);
        members[target].extend(moved);
        alive[light] = false;
    }
    let mut labels = vec![usize::MAX; n];
    let mut next = 0;
    for i in 0..n {
        let c = (0..n).find(|&c| alive[c] && members[c].contains(&i)).unwrap_or(0);
        if labels[i] == usize::MAX {
            for &m in &members[c] {
                labels[m] = next;
            }
            next += 1;
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::l2_normalize;

    fn v(x: f32, y: f32, z: f32) -> Vec<f32> {
        l2_normalize(&[x, y, z])
    }

    #[test]
    fn separates_groups_and_absorbs_strays() {
        let items = vec![v(1.0, 0.1, 0.0), v(0.0, 1.0, 0.1), v(1.0, 0.0, 0.1), v(0.1, 1.0, 0.0), v(0.9, 0.2, 0.1), v(0.5, 0.5, 0.7)];
        let weights = vec![3.0, 3.0, 3.0, 3.0, 3.0, 0.5];
        let labels = agglomerative(&items, &weights, 0.4, 8, 1.0);
        assert_eq!(labels[0], labels[2]);
        assert_eq!(labels[0], labels[4]);
        assert_eq!(labels[1], labels[3]);
        assert_ne!(labels[0], labels[1]);
        assert!(labels[5] == labels[0] || labels[5] == labels[1], "stray absorbed");
        assert_eq!(*labels.iter().max().unwrap(), 1);
    }

    #[test]
    fn caps_cluster_count() {
        let items = vec![v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0), v(0.0, 0.0, 1.0)];
        let labels = agglomerative(&items, &[1.0; 3], 0.1, 2, 0.0);
        assert_eq!(*labels.iter().max().unwrap(), 1);
        assert_eq!(agglomerative(&items[..1], &[1.0], 0.1, 2, 0.0), vec![0]);
    }
}
