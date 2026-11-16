-- 種目マスターの 106 種に、やり方、注意点、コツを入れる (REQ-LIB-002)。
-- 同じ部位区分と同じ器具の種目は共通の文を使い、種目ごとの説明の有無を埋める。
-- 医療上の断定を避け、痛みがある場合は中止して専門家へ相談するよう案内する。

UPDATE master_exercises SET
    instructions = CASE equipment
        WHEN 'barbell' THEN 'バーを肩幅から少し広い位置で握り、胸を張ったままバーをゆっくり下ろして押し上げます。'
        WHEN 'dumbbell' THEN 'ダンベルを両手に持ち、胸を張ったまま肘を曲げて下ろし、胸の筋肉で押し上げます。'
        WHEN 'machine' THEN 'シートに背中をつけ、グリップを握って胸の前へ押し出し、ゆっくり戻します。'
        WHEN 'cable' THEN 'ケーブルのグリップを握り、胸を張ったまま弧を描くように腕を前へ動かします。'
        WHEN 'bodyweight' THEN '肩の下に手を置き、体を一直線に保ったまま肘を曲げて下ろし、押し上げます。'
        ELSE '胸を張り、肩をすくめない姿勢で、胸の筋肉を使って動かします。'
    END,
    cautions = '肩や手首に痛みが出たら中止し、無理に可動域を広げないようにします。',
    tips = '下ろす動作をゆっくり行い、胸の筋肉が伸びる感覚を意識します。'
WHERE body_part = 'chest' AND instructions = '';

UPDATE master_exercises SET
    instructions = CASE equipment
        WHEN 'barbell' THEN 'バーを握り、背筋を伸ばしたまま股関節から上体を倒してバーを引き、肩甲骨を寄せます。'
        WHEN 'dumbbell' THEN 'ダンベルを片手または両手に持ち、背筋を伸ばして引き上げ、肩甲骨を寄せます。'
        WHEN 'machine' THEN '胸をパッドに当て、グリップを引いて肩甲骨を寄せ、ゆっくり戻します。'
        WHEN 'cable' THEN 'ケーブルのグリップを握り、背筋を伸ばしたまま肘を後ろへ引いて肩甲骨を寄せます。'
        WHEN 'bodyweight' THEN 'バーや器具を握り、体を引き上げて顎を近づけ、ゆっくり下ろします。'
        ELSE '背筋を伸ばし、腕ではなく背中の筋肉で引く感覚を意識します。'
    END,
    cautions = '腰を丸めたまま引かないようにし、反動を使わずに動かします。',
    tips = '引いたときに肩甲骨を寄せ、戻すときに肩が前に出ないようにします。'
WHERE body_part = 'back' AND instructions = '';

UPDATE master_exercises SET
    instructions = CASE equipment
        WHEN 'barbell' THEN 'バーを肩の高さで構え、体を一直線に保ったまま頭上へ押し上げます。'
        WHEN 'dumbbell' THEN 'ダンベルを肩の高さに構え、肘を伸ばして頭上へ押し上げます。'
        WHEN 'machine' THEN 'シートに座り、グリップを握って頭上へ押し出し、ゆっくり戻します。'
        WHEN 'cable' THEN 'ケーブルのグリップを握り、肩の高さから横または前へ腕を動かします。'
        WHEN 'bodyweight' THEN '手を床や台に置き、体を一直線に保ったまま肩の筋肉で体を押し上げます。'
        ELSE '肩をすくめず、肘を伸ばす動作で肩の筋肉を使います。'
    END,
    cautions = '肩に痛みがある場合は可動域を下げ、反動を使わないようにします。',
    tips = '腰が反りすぎないように腹圧を保ち、肩の真上へ押し上げます。'
WHERE body_part = 'shoulders' AND instructions = '';

UPDATE master_exercises SET
    instructions = CASE equipment
        WHEN 'barbell' THEN 'バーを手のひらで握り、肘を固定したまま曲げてバーを上げ、ゆっくり下ろします。'
        WHEN 'dumbbell' THEN 'ダンベルを握り、肘を体側に固定して曲げ、ゆっくり下ろします。'
        WHEN 'machine' THEN 'パッドに肘を置き、グリップを握って曲げ、ゆっくり戻します。'
        WHEN 'cable' THEN 'ケーブルのグリップを握り、肘を固定したまま曲げて手を近づけます。'
        WHEN 'bodyweight' THEN 'バーを握って体を引き上げるか、台に手を置いて肘を曲げます。'
        ELSE '肘を体側から離さず、腕の前面の筋肉で曲げます。'
    END,
    cautions = '肘や手首に痛みが出たら中止し、重さを落として動かします。',
    tips = '下ろす動作をゆっくり行い、肘を振らずに前腕を動かします。'
WHERE body_part = 'biceps_forearm' AND instructions = '';

UPDATE master_exercises SET
    instructions = CASE equipment
        WHEN 'barbell' THEN 'バーを肩幅で握り、肘を固定したまま頭上へ押し上げ、ゆっくり下ろします。'
        WHEN 'dumbbell' THEN 'ダンベルを頭の後ろに構え、肘を伸ばして頭上へ押し上げます。'
        WHEN 'machine' THEN 'シートに座り、グリップを握って肘を伸ばし、ゆっくり戻します。'
        WHEN 'cable' THEN 'ケーブルのグリップを握り、肘を固定したまま後ろへ押し出します。'
        WHEN 'bodyweight' THEN '手を台に置き、肘を曲げて体を下ろし、腕の後面で押し上げます。'
        ELSE '肘を体側に固定し、腕の後面の筋肉で伸ばします。'
    END,
    cautions = '肘に痛みが出たら中止し、重さを落として可動域を狭めます。',
    tips = '肘を開きすぎず、伸ばし切る直前にゆっくり動かします。'
WHERE body_part = 'triceps' AND instructions = '';

UPDATE master_exercises SET
    instructions = CASE equipment
        WHEN 'barbell' THEN 'バーを肩に担ぎ、足を肩幅に開いて膝と股関節を曲げ、立ち上がります。'
        WHEN 'dumbbell' THEN 'ダンベルを体側に持ち、膝と股関節を曲げてしゃがみ、立ち上がります。'
        WHEN 'machine' THEN 'フットプレートに足を置き、膝を曲げて戻し、ゆっくり押し出します。'
        WHEN 'cable' THEN 'ケーブルを体に付け、股関節から脚を動かして筋肉を収縮させます。'
        WHEN 'bodyweight' THEN '足を肩幅に開き、膝と股関節を曲げてしゃがみ、立ち上がります。'
        ELSE '膝とつま先の向きを揃え、かかとで床を押して立ち上がります。'
    END,
    cautions = '膝や腰に痛みが出たら中止し、深さと重さを落として動かします。',
    tips = 'しゃがむときに膝が内側へ入らないよう、股関節から動きます。'
WHERE body_part = 'legs' AND instructions = '';

UPDATE master_exercises SET
    instructions = CASE equipment
        WHEN 'machine' THEN 'シートに座り、パッドに体を当てて腹筋を曲げ、ゆっくり戻します。'
        WHEN 'cable' THEN 'ケーブルを握り、腹筋を曲げて上体を丸め、ゆっくり戻します。'
        WHEN 'bodyweight' THEN '仰向けやプランクの姿勢から、腰を反らさずに腹筋を収縮させます。'
        ELSE '腰を反らさず、息を吐きながら腹筋を縮めます。'
    END,
    cautions = '腰に痛みが出たら中止し、反動を使わずに動かします。',
    tips = '首を引っ張らず、みぞおちを骨盤へ近づける感覚で動きます。'
WHERE body_part = 'abs' AND instructions = '';

UPDATE master_exercises SET
    instructions = CASE equipment
        WHEN 'machine' THEN 'マシンに合わせて姿勢を整え、決められた動作を一定の速さで行います。'
        WHEN 'bodyweight' THEN '姿勢を一直線に保ち、無理のない範囲で全身を使う動作を行います。'
        ELSE '姿勢を整え、呼吸を止めずに一定の速さで動作します。'
    END,
    cautions = '痛みや息苦しさがある場合は中止し、体調に合わせて強度を下げます。',
    tips = '動作を急がず、フォームが崩れたら強度を落とします。'
WHERE body_part = 'other' AND instructions = '';

-- 種目名が固有のフォームを持つ代表的な種目は、共通の文を上書きする。
UPDATE master_exercises SET
    instructions = 'バーを肩甲骨の下に担ぎ、足を肩幅に開いて胸を張り、膝と股関節を曲げてしゃがみ、かかとで床を押して立ち上がります。',
    cautions = '膝が内側へ入らないようにし、腰が丸まる深さまでしゃがみません。',
    tips = '目線を前に保ち、腹圧を高めたまま上下します。'
WHERE name = 'スクワット';

UPDATE master_exercises SET
    instructions = 'ベンチに仰向けになり、バーを肩幅より少し広く握って胸の上へ下ろし、胸を張ったまま押し上げます。',
    cautions = '肩をすくめたまま無理に下ろさず、補助者を置いて安全に実施します。',
    tips = '肩甲骨を寄せてベンチに固定し、足で床を押します。'
WHERE name = 'ベンチプレス';

UPDATE master_exercises SET
    instructions = 'バーを握り、背筋を伸ばしたまま股関節から上体を倒し、バーを脛に沿って引き上げて立ち上がります。',
    cautions = '腰を丸めたまま引き上げず、重さを上げるときは補助者を置きます。',
    tips = 'バーを体から離さず、股関節と膝を同時に伸ばします。'
WHERE name = 'デッドリフト';

UPDATE master_exercises SET
    instructions = 'バーを肩幅で握り、胸を張ってバーを鎖骨の高さまで引き上げ、肩甲骨を寄せてからゆっくり下ろします。',
    cautions = '反動を使わず、腰を丸めないようにします。',
    tips = '肘を後ろへ引く意識で、背中の筋肉を収縮させます。'
WHERE name = 'ベントオーバーロウ';

UPDATE master_exercises SET
    instructions = 'バーを肩幅で握り、肘を伸ばしたまま肩の高さまで引き上げ、肩の筋肉で押し上げます。',
    cautions = '腰を反りすぎないようにし、肩に痛みがあれば可動域を下げます。',
    tips = 'バーを顎の下へ通す意識で、体の前で押し上げます。'
WHERE name = 'オーバーヘッドプレス';

UPDATE master_exercises SET
    instructions = '頭の後ろに組んだ手を添え、膝を曲げて上体を起こし、腹筋を収縮させてゆっくり戻します。',
    cautions = '首を引っ張らず、腰に痛みが出たら中止します。',
    tips = '息を吐きながらみぞおちを骨盤へ近づけます。'
WHERE name = 'クランチ';

UPDATE master_exercises SET
    instructions = '前腕とつま先で体を支え、頭からかかとまでを一直線に保ちます。',
    cautions = '腰が落ちたり反ったりしたら中止し、膝を床につけて強度を下げます。',
    tips = '息を止めず、腹筋と臀部に力を入れて姿勢を保ちます。'
WHERE name = 'プランク';
