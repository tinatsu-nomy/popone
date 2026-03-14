// Unity Humanoid パラメータダンプスクリプト
//
// 使い方:
// 1. このファイルを Unity プロジェクトの Editor/ フォルダに配置
// 2. Humanoid アバター (VRM モデル等) を Scene に配置して選択
// 3. メニュー → Tools → Dump Humanoid Params を実行
// 4. 出力された JSON ファイルを anim2vrma.py の --params オプションに指定
//
// 出力される JSON には以下が含まれる:
// - 全ボーンの preQ, postQ, sign (アバター固有)
// - MuscleDefaultMin/Max (全95マッスル)
// - MuscleFromBone マッピング
// - MuscleName 配列

using System.Collections.Generic;
using System.IO;
using System.Reflection;
using UnityEditor;
using UnityEngine;

public class DumpHumanoidParams
{
    [MenuItem("Tools/Dump Humanoid Params")]
    static void Execute()
    {
        var go = Selection.activeGameObject;
        if (go == null)
        {
            Debug.LogError("Humanoid アバターを選択してください");
            return;
        }

        var animator = go.GetComponent<Animator>();
        if (animator == null || !animator.isHuman)
        {
            Debug.LogError("選択されたオブジェクトに Humanoid Animator がありません");
            return;
        }

        var avatar = animator.avatar;
        var result = new Dictionary<string, object>();

        // Reflection で private メソッドを取得
        var getPreRotation = typeof(Avatar).GetMethod("GetPreRotation",
            BindingFlags.NonPublic | BindingFlags.Instance);
        var getPostRotation = typeof(Avatar).GetMethod("GetPostRotation",
            BindingFlags.NonPublic | BindingFlags.Instance);
        var getLimitSign = typeof(Avatar).GetMethod("GetLimitSign",
            BindingFlags.NonPublic | BindingFlags.Instance);

        // ボーンパラメータ
        var bones = new List<Dictionary<string, object>>();
        for (int i = 0; i < HumanTrait.BoneCount; i++)
        {
            var bone = new Dictionary<string, object>();
            bone["index"] = i;
            bone["name"] = HumanTrait.BoneName[i];

            var humanBone = (HumanBodyBones)i;
            var transform = animator.GetBoneTransform(humanBone);
            bone["present"] = transform != null;

            if (transform != null)
            {
                var preQ = (Quaternion)getPreRotation.Invoke(avatar, new object[] { humanBone });
                var postQ = (Quaternion)getPostRotation.Invoke(avatar, new object[] { humanBone });
                var sign = GetLimitSignFull(avatar, humanBone, getLimitSign);

                bone["preQ"] = new float[] { preQ.x, preQ.y, preQ.z, preQ.w };
                bone["postQ"] = new float[] { postQ.x, postQ.y, postQ.z, postQ.w };
                bone["sign"] = new float[] { sign.x, sign.y, sign.z };

                // ローカル回転 (レスト回転)
                bone["localRotation"] = new float[] {
                    transform.localRotation.x, transform.localRotation.y,
                    transform.localRotation.z, transform.localRotation.w
                };
                bone["localPosition"] = new float[] {
                    transform.localPosition.x, transform.localPosition.y,
                    transform.localPosition.z
                };
            }

            // MuscleFromBone
            bone["muscles"] = new int[] {
                HumanTrait.MuscleFromBone(i, 0),
                HumanTrait.MuscleFromBone(i, 1),
                HumanTrait.MuscleFromBone(i, 2)
            };

            bones.Add(bone);
        }
        result["bones"] = bones;

        // マッスルパラメータ
        var muscles = new List<Dictionary<string, object>>();
        for (int i = 0; i < HumanTrait.MuscleCount; i++)
        {
            var muscle = new Dictionary<string, object>();
            muscle["index"] = i;
            muscle["name"] = HumanTrait.MuscleName[i];
            muscle["defaultMin"] = HumanTrait.GetMuscleDefaultMin(i);
            muscle["defaultMax"] = HumanTrait.GetMuscleDefaultMax(i);
            muscles.Add(muscle);
        }
        result["muscles"] = muscles;

        // メタデータ
        result["avatarName"] = avatar.name;
        result["gameObjectName"] = go.name;
        result["humanScale"] = animator.humanScale;

        // JSON 出力
        var json = JsonUtility.ToJson(new JsonWrapper { data = result }, true);
        // JsonUtility は Dictionary を直接シリアライズできないので MiniJSON 的な手動変換
        var jsonStr = DictToJson(result, 0);

        var path = EditorUtility.SaveFilePanel(
            "Humanoid パラメータを保存", "", go.name + "_humanoid_params.json", "json");

        if (!string.IsNullOrEmpty(path))
        {
            File.WriteAllText(path, jsonStr);
            Debug.Log($"パラメータを保存しました: {path}");
        }
    }

    // lyuma 氏の GetLimitSign 改良版
    static Vector3 GetLimitSignFull(Avatar avatar, HumanBodyBones humanBone, MethodInfo getLimitSign)
    {
        var sign = Vector3.zero;
        for (var b = humanBone; (int)b >= 0;)
        {
            var s = (Vector3)getLimitSign.Invoke(avatar, new object[] { b });
            for (int i = 0; i < 3; i++)
            {
                if (HumanTrait.MuscleFromBone((int)b, i) < 0)
                    s[i] = 0;
                else if (sign[i] == 0)
                    sign[i] = s[i];
            }
            if (s.x * s.y * s.z != 0)
            {
                for (int i = 0; i < 3 && (sign.x * sign.y * sign.z) != (s.x * s.y * s.z); i++)
                    if (HumanTrait.MuscleFromBone((int)humanBone, i) < 0)
                        sign[i] *= -1;
                return sign;
            }
            b = b == HumanBodyBones.LeftShoulder ? HumanBodyBones.LeftUpperArm :
                b == HumanBodyBones.RightShoulder ? HumanBodyBones.RightUpperArm :
                (HumanBodyBones)HumanTrait.GetParentBone((int)b);
        }
        return Vector3.one;
    }

    // 簡易 JSON シリアライザ
    static string DictToJson(object obj, int indent)
    {
        var pad = new string(' ', indent * 2);
        var pad1 = new string(' ', (indent + 1) * 2);

        if (obj is Dictionary<string, object> dict)
        {
            var parts = new List<string>();
            foreach (var kv in dict)
                parts.Add($"{pad1}\"{kv.Key}\": {DictToJson(kv.Value, indent + 1)}");
            return "{\n" + string.Join(",\n", parts) + "\n" + pad + "}";
        }
        if (obj is List<Dictionary<string, object>> list)
        {
            var parts = new List<string>();
            foreach (var item in list)
                parts.Add(pad1 + DictToJson(item, indent + 1));
            return "[\n" + string.Join(",\n", parts) + "\n" + pad + "]";
        }
        if (obj is float[] fa)
        {
            var parts = new List<string>();
            foreach (var f in fa) parts.Add(f.ToString("G9"));
            return "[" + string.Join(", ", parts) + "]";
        }
        if (obj is int[] ia)
        {
            var parts = new List<string>();
            foreach (var i in ia) parts.Add(i.ToString());
            return "[" + string.Join(", ", parts) + "]";
        }
        if (obj is float f2) return f2.ToString("G9");
        if (obj is int i2) return i2.ToString();
        if (obj is bool b2) return b2 ? "true" : "false";
        if (obj is string s2) return $"\"{s2}\"";
        return obj?.ToString() ?? "null";
    }

    class JsonWrapper
    {
        public object data;
    }
}
