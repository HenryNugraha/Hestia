use serde::Deserialize;

const MODEL_SCHEMA: &str = "hestia-native-hgb-model-v1";
const RAW_FEATURE_COUNT: usize = 92;

#[derive(Debug, Clone, PartialEq)]
pub enum MatcherError {
    Json(String),
    SchemaMismatch(String),
    InvalidModel(String),
    WrongFeatureCount { expected: usize, actual: usize },
}

impl std::fmt::Display for MatcherError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(message) => write!(formatter, "invalid model JSON: {message}"),
            Self::SchemaMismatch(message) => write!(formatter, "model schema mismatch: {message}"),
            Self::InvalidModel(message) => write!(formatter, "invalid model: {message}"),
            Self::WrongFeatureCount { expected, actual } => {
                write!(
                    formatter,
                    "wrong feature count: expected {expected}, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for MatcherError {}

#[derive(Debug, Clone, Deserialize)]
pub struct NativeModel {
    schema: String,
    model_variant: String,
    raw_feature_count: usize,
    raw_feature_names: Vec<String>,
    output_feature_count: usize,
    output_feature_names: Vec<String>,
    imputer_medians: Vec<f64>,
    missing_indicator_indexes: Vec<usize>,
    scaler_means: Vec<f64>,
    scaler_scales: Vec<f64>,
    baseline_prediction: f64,
    learning_rate: f64,
    leaf_values_include_learning_rate: bool,
    tree_count: usize,
    trees: Vec<Tree>,
    threshold: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct Tree {
    nodes: Vec<Node>,
}

#[derive(Debug, Clone, Deserialize)]
struct Node {
    value: f64,
    is_leaf: bool,
    is_categorical: bool,
    feature_idx: Option<usize>,
    threshold: Option<f64>,
    missing_go_to_left: Option<bool>,
    left: Option<usize>,
    right: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    pub raw_score: f64,
    pub probability: f64,
    pub accepted: bool,
}

impl NativeModel {
    pub fn from_json_str(value: &str) -> Result<Self, MatcherError> {
        let model: Self =
            serde_json::from_str(value).map_err(|error| MatcherError::Json(error.to_string()))?;
        model.validate()?;
        Ok(model)
    }

    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    pub fn score_raw(&self, raw_features: &[Option<f64>]) -> Result<Score, MatcherError> {
        if raw_features.len() != RAW_FEATURE_COUNT {
            return Err(MatcherError::WrongFeatureCount {
                expected: RAW_FEATURE_COUNT,
                actual: raw_features.len(),
            });
        }
        let transformed = self.transform(raw_features);
        Ok(self.score_transformed(&transformed))
    }

    fn validate(&self) -> Result<(), MatcherError> {
        if self.schema != MODEL_SCHEMA {
            return Err(MatcherError::SchemaMismatch(format!(
                "expected {MODEL_SCHEMA}, got {}",
                self.schema
            )));
        }
        if self.model_variant != "ablation92" {
            return Err(MatcherError::InvalidModel(format!(
                "expected ablation92, got {}",
                self.model_variant
            )));
        }
        if self.raw_feature_count != RAW_FEATURE_COUNT
            || self.raw_feature_names.len() != RAW_FEATURE_COUNT
        {
            return Err(MatcherError::InvalidModel(format!(
                "raw feature count must be {RAW_FEATURE_COUNT}"
            )));
        }
        if self.output_feature_count != self.output_feature_names.len()
            || self.imputer_medians.len() != RAW_FEATURE_COUNT
            || self.scaler_means.len() != self.output_feature_count
            || self.scaler_scales.len() != self.output_feature_count
        {
            return Err(MatcherError::InvalidModel(
                "preprocessor lengths do not match feature counts".to_owned(),
            ));
        }
        if self
            .missing_indicator_indexes
            .iter()
            .any(|index| *index >= RAW_FEATURE_COUNT)
        {
            return Err(MatcherError::InvalidModel(
                "missing indicator index is out of range".to_owned(),
            ));
        }
        let mut indicators = self.missing_indicator_indexes.clone();
        indicators.sort_unstable();
        indicators.dedup();
        if indicators.len() != self.missing_indicator_indexes.len() {
            return Err(MatcherError::InvalidModel(
                "duplicate missing indicator index".to_owned(),
            ));
        }
        for value in self
            .imputer_medians
            .iter()
            .chain(self.scaler_means.iter())
            .chain(self.scaler_scales.iter())
        {
            if !value.is_finite() {
                return Err(MatcherError::InvalidModel(
                    "non-finite preprocessing parameter".to_owned(),
                ));
            }
        }
        if self.scaler_scales.iter().any(|scale| *scale <= 0.0)
            || !self.baseline_prediction.is_finite()
            || !self.learning_rate.is_finite()
            || !self.threshold.is_finite()
            || !self.leaf_values_include_learning_rate
            || self.tree_count != self.trees.len()
        {
            return Err(MatcherError::InvalidModel(
                "invalid model scalar or tree metadata".to_owned(),
            ));
        }
        if self.trees.is_empty() {
            return Err(MatcherError::InvalidModel("model has no trees".to_owned()));
        }
        for tree in &self.trees {
            if tree.nodes.is_empty() {
                return Err(MatcherError::InvalidModel("tree has no nodes".to_owned()));
            }
            for (index, node) in tree.nodes.iter().enumerate() {
                if !node.value.is_finite() {
                    return Err(MatcherError::InvalidModel(
                        "tree contains non-finite value".to_owned(),
                    ));
                }
                if node.is_categorical {
                    return Err(MatcherError::InvalidModel(
                        "categorical tree node is unsupported".to_owned(),
                    ));
                }
                if node.is_leaf {
                    if node.feature_idx.is_some()
                        || node.threshold.is_some()
                        || node.missing_go_to_left.is_some()
                        || node.left.is_some()
                        || node.right.is_some()
                    {
                        return Err(MatcherError::InvalidModel(format!(
                            "leaf node {index} carries split fields"
                        )));
                    }
                } else {
                    let feature = node.feature_idx.ok_or_else(|| {
                        MatcherError::InvalidModel("split node lacks feature".to_owned())
                    })?;
                    let threshold = node.threshold.ok_or_else(|| {
                        MatcherError::InvalidModel("split node lacks threshold".to_owned())
                    })?;
                    let left = node.left.ok_or_else(|| {
                        MatcherError::InvalidModel("split node lacks left child".to_owned())
                    })?;
                    let right = node.right.ok_or_else(|| {
                        MatcherError::InvalidModel("split node lacks right child".to_owned())
                    })?;
                    if feature >= self.output_feature_count
                        || !threshold.is_finite()
                        || left >= tree.nodes.len()
                        || right >= tree.nodes.len()
                    {
                        return Err(MatcherError::InvalidModel(
                            "split node references invalid data".to_owned(),
                        ));
                    }
                    if node.missing_go_to_left.is_none() {
                        return Err(MatcherError::InvalidModel(
                            "split node lacks missing direction".to_owned(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn transform(&self, raw_features: &[Option<f64>]) -> Vec<f64> {
        let mut values = Vec::with_capacity(self.output_feature_count);
        let mut missing = vec![false; RAW_FEATURE_COUNT];
        for (index, value) in raw_features.iter().enumerate() {
            let number = value.filter(|number| number.is_finite());
            match number {
                Some(number) => values.push(number),
                None => {
                    missing[index] = true;
                    values.push(self.imputer_medians[index]);
                }
            }
        }
        for index in &self.missing_indicator_indexes {
            values.push(if missing[*index] { 1.0 } else { 0.0 });
        }
        for ((value, mean), scale) in values
            .iter_mut()
            .zip(self.scaler_means.iter())
            .zip(self.scaler_scales.iter())
        {
            *value = (*value - *mean) / *scale;
        }
        values
    }

    fn score_transformed(&self, values: &[f64]) -> Score {
        let mut raw_score = self.baseline_prediction;
        for tree in &self.trees {
            raw_score += walk_tree(tree, values);
        }
        let probability = stable_sigmoid(raw_score);
        Score {
            raw_score,
            probability,
            accepted: probability > self.threshold,
        }
    }
}

fn walk_tree(tree: &Tree, values: &[f64]) -> f64 {
    let mut index = 0usize;
    loop {
        let node = &tree.nodes[index];
        if node.is_leaf {
            return node.value;
        }
        let feature = node.feature_idx.expect("validated split feature");
        let threshold = node.threshold.expect("validated split threshold");
        let go_left = match values[feature].is_finite() {
            true => values[feature] <= threshold,
            false => node
                .missing_go_to_left
                .expect("validated missing direction"),
        };
        index = if go_left {
            node.left.expect("validated left child")
        } else {
            node.right.expect("validated right child")
        };
    }
}

pub fn stable_sigmoid(raw_score: f64) -> f64 {
    if raw_score >= 0.0 {
        1.0 / (1.0 + (-raw_score).exp())
    } else {
        let exp_raw = raw_score.exp();
        exp_raw / (1.0 + exp_raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(gamebanana_matcher_fixtures)]
    use serde::Deserialize;

    const MODEL_JSON: &str = include_str!("model.json");
    #[cfg(gamebanana_matcher_fixtures)]
    const FIXTURES_JSON: &str = include_str!("tests/fixtures/model-parity-fixtures.json");

    #[cfg(gamebanana_matcher_fixtures)]
    #[derive(Debug, Deserialize)]
    struct Fixtures {
        model_sha256: String,
        cases: Vec<FixtureCase>,
    }

    #[cfg(gamebanana_matcher_fixtures)]
    #[derive(Debug, Deserialize)]
    struct FixtureCase {
        raw_features: Vec<Option<f64>>,
        expected_raw_score: f64,
        expected_probability: f64,
        expected_accepted: bool,
    }

    #[cfg(gamebanana_matcher_fixtures)]
    #[test]
    fn real_fixtures_match_sklearn_to_double_precision() {
        let model = NativeModel::from_json_str(MODEL_JSON).unwrap();
        let fixtures: Fixtures = serde_json::from_str(FIXTURES_JSON).unwrap();
        assert!(!fixtures.model_sha256.is_empty());
        for fixture in fixtures.cases {
            let score = model.score_raw(&fixture.raw_features).unwrap();
            assert!((score.raw_score - fixture.expected_raw_score).abs() <= 1e-12);
            assert!((score.probability - fixture.expected_probability).abs() <= 1e-12);
            assert_eq!(score.accepted, fixture.expected_accepted);
        }
    }

    #[test]
    fn wrong_feature_count_is_rejected() {
        let model = NativeModel::from_json_str(MODEL_JSON).unwrap();
        let error = model.score_raw(&[]).unwrap_err();
        assert!(matches!(
            error,
            MatcherError::WrongFeatureCount {
                expected: 92,
                actual: 0
            }
        ));
    }

    #[test]
    fn schema_mismatch_is_rejected() {
        let invalid = MODEL_JSON.replacen("hestia-native-hgb-model-v1", "other-schema", 1);
        let error = NativeModel::from_json_str(&invalid).unwrap_err();
        assert!(matches!(error, MatcherError::SchemaMismatch(_)));
    }

    #[test]
    fn nonfinite_input_uses_missing_imputation() {
        let model = NativeModel::from_json_str(MODEL_JSON).unwrap();
        let missing = vec![None; 92];
        let explicit_nan = vec![Some(f64::NAN); 92];
        assert_eq!(
            model.score_raw(&missing).unwrap(),
            model.score_raw(&explicit_nan).unwrap()
        );
    }

    #[test]
    fn stable_sigmoid_handles_extreme_scores() {
        assert_eq!(stable_sigmoid(0.0), 0.5);
        assert!(stable_sigmoid(1000.0) > 0.999999);
        assert!(stable_sigmoid(-1000.0) < 0.000001);
    }
}
