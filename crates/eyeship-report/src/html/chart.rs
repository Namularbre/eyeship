//! Evolution of the findings over saved reports, as an inline SVG: no
//! library, no JavaScript (plan §5). The history table holds the same data.

use eyeship_domain::Summary;

const WIDTH: usize = 600;
const HEIGHT: usize = 160;
const PAD: usize = 8;

/// One polyline per severity over `points`, oldest first. `None` below two
/// points, where a line says nothing.
pub fn evolution(points: &[Summary]) -> Option<String> {
    if points.len() < 2 {
        return None;
    }
    let max = points
        .iter()
        .map(|s| s.error.max(s.warning).max(s.info))
        .max()
        .unwrap_or(0)
        .max(1);
    let step = (WIDTH - 2 * PAD) as f64 / (points.len() - 1) as f64;
    let y =
        |count: usize| PAD as f64 + (HEIGHT - 2 * PAD) as f64 * (1.0 - count as f64 / max as f64);
    let line = |class: &str, count: fn(&Summary) -> usize| {
        let coordinates: Vec<String> = points
            .iter()
            .enumerate()
            .map(|(index, summary)| {
                format!(
                    "{:.1},{:.1}",
                    PAD as f64 + step * index as f64,
                    y(count(summary))
                )
            })
            .collect();
        format!(
            "<polyline class=\"line {class}\" points=\"{}\"/>",
            coordinates.join(" ")
        )
    };
    Some(format!(
        "<svg class=\"chart\" viewBox=\"0 0 {WIDTH} {HEIGHT}\" role=\"img\" \
         aria-labelledby=\"chart-title\" preserveAspectRatio=\"none\">\
         <title id=\"chart-title\">Findings per severity over the last {} saved reports, \
         from 0 to {max}. The table below lists the same values.</title>\
         <line class=\"axis\" x1=\"{PAD}\" y1=\"{bottom}\" x2=\"{right}\" y2=\"{bottom}\"/>\
         {}{}{}</svg>\
         <ul class=\"legend\"><li>— errors</li><li>- - warnings</li><li>··· info</li><li>max {max}</li></ul>",
        points.len(),
        line("error", |s| s.error),
        line("warning", |s| s.warning),
        line("info", |s| s.info),
        bottom = HEIGHT - PAD,
        right = WIDTH - PAD,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(error: usize, warning: usize, info: usize) -> Summary {
        Summary {
            error,
            warning,
            info,
        }
    }

    #[test]
    fn needs_two_points() {
        assert_eq!(evolution(&[]), None);
        assert_eq!(evolution(&[summary(1, 0, 0)]), None);
    }

    #[test]
    fn draws_one_line_per_severity_scaled_to_the_maximum() {
        let svg = evolution(&[summary(2, 0, 1), summary(0, 4, 1)]).expect("a chart");
        assert_eq!(svg.matches("<polyline").count(), 3);
        // errors: from 2/4 of the height to the bottom.
        assert!(
            svg.contains("class=\"line error\" points=\"8.0,80.0 592.0,152.0\""),
            "{svg}"
        );
        assert!(
            svg.contains("class=\"line warning\" points=\"8.0,152.0 592.0,8.0\""),
            "{svg}"
        );
        assert!(svg.contains("from 0 to 4"));
    }

    #[test]
    fn an_all_zero_history_stays_on_the_axis() {
        let svg = evolution(&[summary(0, 0, 0), summary(0, 0, 0)]).expect("a chart");
        assert!(svg.contains("points=\"8.0,152.0 592.0,152.0\""));
    }
}
