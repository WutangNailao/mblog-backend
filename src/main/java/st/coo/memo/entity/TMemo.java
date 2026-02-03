package st.coo.memo.entity;

import com.mybatisflex.annotation.Id;
import com.mybatisflex.annotation.KeyType;
import com.mybatisflex.annotation.Table;
import lombok.Getter;
import lombok.Setter;

import java.io.Serializable;
import java.sql.Timestamp;


@Setter
@Getter
@Table(value = "t_memo")
public class TMemo implements Serializable {

    
    @Id(keyType = KeyType.Auto)
    private Integer id;

    
    private Integer userId;

    
    private String content;

    
    private String tags;

    
    private String visibility;

    
    private String status;

    
    private Timestamp created;

    
    private Timestamp updated;

    
    private Integer priority;

    
    private Integer commentCount;

    
    private Integer likeCount;

    
    private Integer enableComment;

    
    private Integer viewCount;
    private String source;

}
